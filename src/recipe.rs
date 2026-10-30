use std::collections::HashSet;
use std::time::Duration;

use base64::Engine;
use recipe_scraper::Extract;
use recipe_scraper::SchemaOrgEntry;

use crate::error::AppError;
use crate::model::NewIngredientLine;

/// Output of a recipe parse — a meal-shaped draft, not persisted.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ImportDraft {
    pub name: String,
    pub ingredients: Vec<NewIngredientLine>,
    pub instructions: String,
    /// Base64-encoded JPEG bytes if an image was found and downloaded; None otherwise.
    /// Only populated by `fetch_and_parse` (URL mode). Always `None` for `parse_recipe` (paste mode).
    pub image_base64: Option<String>,
    #[serde(default)]
    pub portions: Option<i32>,
    #[serde(default)]
    pub source_url: Option<String>,
}

/// Parse a recipe from raw HTML or JSON-LD text. No network fetch.
/// `image_base64` is always `None` in the returned draft (paste mode cannot download).
pub fn parse_recipe(text: &str) -> Result<ImportDraft, AppError> {
    let (draft, _image_url) = parse_recipe_with_image_url(text)?;
    Ok(ImportDraft {
        name: draft.name,
        ingredients: draft.ingredients,
        instructions: draft.instructions,
        image_base64: None,
        portions: draft.portions,
        source_url: None,
    })
}

/// Fetch a URL and return the HTML body as a `String`. Used by both
/// [`fetch_and_parse`] (recipe-scraper path) and [`import_from_llm`]
/// (LLM URL expansion path).
pub async fn fetch_page_html(url: &str) -> Result<String, AppError> {
    let parsed_url =
        reqwest::Url::parse(url).map_err(|_| AppError::BadRequest("invalid URL".into()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    let resp = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to fetch page: {e}")))?;

    if !resp.status().is_success() {
        let status = resp.status();
        // 403 and 429 are how bot protection and rate limiting answer
        // automated clients. The body is a challenge or error page, not a
        // recipe, so report the actual cause instead of a parser error.
        if status == reqwest::StatusCode::FORBIDDEN
            || status == reqwest::StatusCode::TOO_MANY_REQUESTS
        {
            return Err(AppError::Import(
                "the site refused automated access".into(),
                "import_site_blocked",
            ));
        }
        return Err(AppError::BadRequest(format!(
            "fetch returned HTTP {status}"
        )));
    }

    if let Some(len) = resp.content_length()
        && len > 2_000_000
    {
        return Err(AppError::BadRequest("page too large (max 2MB)".into()));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to read page body: {e}")))?;

    std::str::from_utf8(&bytes)
        .map(|s| s.to_string())
        .map_err(|_| AppError::BadRequest("page is not valid UTF-8".into()))
}
/// Fetch a URL server-side, then parse. Image download is best-effort.
pub async fn fetch_and_parse(url: &str) -> Result<ImportDraft, AppError> {
    // Cookidoo hides preparation steps from anonymous visitors on its public
    // pages; its mobile API serves the full recipe, so prefer it and fall
    // back to generic HTML parsing when it fails.
    if let Some(draft) = try_fetch_cookidoo(url).await {
        return Ok(draft);
    }

    let html = fetch_page_html(url).await?;
    let (mut draft, image_url) = parse_recipe_with_image_url(&html)?;

    // Image download (best-effort) — needs its own client
    if let Some(img_url) = image_url {
        let img_url = resolve_url(&img_url, url).unwrap_or(img_url);
        if let Ok(client) = reqwest::Client::builder()
            .timeout(Duration::from_secs(30))
            .connect_timeout(Duration::from_secs(10))
            .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
            .build()
            && let Some(jpeg_bytes) = try_download_image(&client, &img_url).await {
                let b64 = base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes);
                draft.image_base64 = Some(b64);
            }
    }

    Ok(draft)
}

/// Try to fetch a full recipe from the Cookidoo mobile API. Returns `None`
/// for non-Cookidoo URLs and for any API failure, so callers can fall back
/// to generic HTML parsing.
async fn try_fetch_cookidoo(url: &str) -> Option<ImportDraft> {
    let api_url = cookidoo_api_url(url)?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .ok()?;

    let resp = client
        .get(&api_url)
        .header("accept", "application/vnd.vorwerk.recipe.embedded.hal+json")
        .send()
        .await
        .ok()?;

    if !resp.status().is_success() {
        tracing::warn!(url = %api_url, status = %resp.status(), "cookidoo API fetch failed; falling back to HTML parsing");
        return None;
    }

    let json: serde_json::Value = match resp.json().await {
        Ok(json) => json,
        Err(e) => {
            tracing::warn!(url = %api_url, error = %e, "cookidoo API returned invalid JSON; falling back to HTML parsing");
            return None;
        }
    };

    let mut draft = match draft_from_cookidoo_json(&json) {
        Some(draft) => draft,
        None => {
            tracing::warn!(url = %api_url, "cookidoo API payload has no usable recipe; falling back to HTML parsing");
            return None;
        }
    };

    if let Some(image_url) = cookidoo_image_url(&json)
        && let Some(jpeg_bytes) = try_download_image(&client, &image_url).await
    {
        draft.image_base64 = Some(base64::engine::general_purpose::STANDARD.encode(&jpeg_bytes));
    }

    Some(draft)
}

/// Map a Cookidoo recipe page URL to the mobile API URL that serves the
/// complete recipe (including preparation steps).
///
/// `https://cookidoo.de/recipes/recipe/de-DE/r47931`
/// -> `https://de.tmmobile.vorwerk-digital.com/recipes/recipe/de-DE/r47931`
///
/// Most markets name the API host after the single host label under
/// `cookidoo.` (`de`, `fr`, `ca`, …), but a few use a different label for a
/// multi-label host. Those are listed explicitly (each verified against the
/// live page and mobile API); every other host shape yields `None` so the
/// caller can use the generic HTML parser.
pub(crate) fn cookidoo_api_url(page_url: &str) -> Option<String> {
    let parsed = reqwest::Url::parse(page_url).ok()?;
    let host = parsed.host_str()?.to_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let suffix = host.strip_prefix("cookidoo.")?;
    let market = match suffix {
        "co.uk" => "gb",
        "com.au" => "au",
        "com.tr" => "tr",
        // `cookidoo.us` 307-redirects to `cookidoo.thermomix.com` with the
        // path preserved; the US mobile API answers on the `us` label.
        "thermomix.com" => "us",
        // The Vietnamese page is `cookidoo.thermomix.vn` and the mobile API
        // answers on the `vn` label (vi-VN r939387). `cookidoo.com.cn` and
        // `cookidoo.international` are deliberately absent: their pages exist
        // but `cn.`/`international.tmmobile...` answer 502, so they fall back
        // to HTML parsing.
        "thermomix.vn" => "vn",
        // `cookidoo.international` resolves on the web (307) but its mobile
        // host answers 502, so it must not resolve to an API URL.
        "international" => return None,
        other if !other.is_empty() && other.chars().all(|c| c.is_ascii_alphanumeric()) => other,
        _ => return None,
    };

    let segments: Vec<&str> = parsed
        .path_segments()?
        .filter(|segment| !segment.is_empty())
        .collect();
    let [first, second, locale, recipe_id] = segments[..] else {
        return None;
    };
    if first != "recipes" || second != "recipe" {
        return None;
    }
    if locale.len() < 2
        || locale.len() > 10
        || !locale
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-')
    {
        return None;
    }
    let digits = recipe_id.strip_prefix('r')?;
    if digits.is_empty() || !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }

    Some(format!(
        "https://{market}.tmmobile.vorwerk-digital.com/recipes/recipe/{locale}/{recipe_id}"
    ))
}

/// Build a draft from a Cookidoo mobile API recipe payload. Returns `None`
/// when the payload has no usable title.
pub(crate) fn draft_from_cookidoo_json(json: &serde_json::Value) -> Option<ImportDraft> {
    let name = json.get("title")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let mut ingredients = Vec::new();
    // The payload language decides ambiguous units ("dose"/"doses") and, for a
    // zero amount, whether a unit word is qualitative; fall back to `locale`
    // when `language` is absent or blank.
    let language = ["language", "locale"].iter().find_map(|key| {
        json.get(*key)
            .and_then(|value| value.as_str())
            .map(str::trim)
            .filter(|language| !language.is_empty())
    });
    // Cookidoo repeats ingredients across groups ("Milch" in "Hefeteig" and
    // again in "Füllung und Fertigstellung"). Two lines resolving to the same
    // ingredient would make `db::set_meal_ingredients` fail on the
    // `meal_ingredients` primary key and the imported meal unsaveable, so later
    // duplicates are renamed rather than dropped.
    let mut seen_ingredient_names = HashSet::new();
    if let Some(groups) = json
        .get("recipeIngredientGroups")
        .and_then(|g| g.as_array())
    {
        for group in groups {
            let group_title = group
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .trim();
            let Some(items) = group.get("recipeIngredients").and_then(|i| i.as_array()) else {
                continue;
            };
            for item in items {
                if let Some(line) = cookidoo_ingredient_line(item, language) {
                    ingredients.push(disambiguate_cookidoo_ingredient(
                        line,
                        group_title,
                        &mut seen_ingredient_names,
                    ));
                }
            }
        }
    }

    let mut steps = Vec::new();
    if let Some(groups) = json.get("recipeStepGroups").and_then(|g| g.as_array()) {
        for group in groups {
            let Some(items) = group.get("recipeSteps").and_then(|s| s.as_array()) else {
                continue;
            };
            for item in items {
                if let Some(text) = item.get("formattedText").and_then(|t| t.as_str()) {
                    let text = text.trim();
                    if !text.is_empty() {
                        steps.push(text.to_string());
                    }
                }
            }
        }
    }
    let instructions = decode_html_entities(&sanitize_instructions(&steps.join("\n")));

    // `servingSize.quantity.value` is only a serving count when `unitNotation`
    // denotes portions; a size ("650 g", "1 Flasche") would otherwise import as
    // "Serves 650".
    let serving_unit = json
        .get("servingSize")
        .and_then(|s| s.get("unitNotation"))
        .and_then(|u| u.as_str());
    let portions = json
        .get("servingSize")
        .and_then(|s| s.get("quantity"))
        .and_then(|q| q.get("value"))
        .and_then(|v| v.as_i64())
        .and_then(|value| i32::try_from(value).ok())
        .filter(|value| (1..=10000).contains(value))
        .filter(|_| cookidoo_serving_unit_is_portions(serving_unit, language));

    Some(ImportDraft {
        name,
        ingredients,
        instructions,
        image_base64: None,
        portions,
        source_url: None,
    })
}

/// Unit words that name a *measure* (mass, volume, spoon, cup, pinch) or a
/// *container* (bottle, jar, glass, package) rather than a serving count. It is
/// a closed, small class, so this list is the rejection set: every other
/// `servingSize.unitNotation` is treated as a serving count. Count nouns are an
/// open class across the ~20 markets Cookidoo serves (`kromek`, `κομμάτια`,
/// `sneetjes`, `unidades`, …), so recognising *them* by enumeration missed
/// recipes over and over.
///
/// The unit is lower-cased, minus a trailing dot, and split on whitespace
/// before the lookup, so one entry covers German capitalisation (`Gramm`,
/// `Liter`, `Gläser`) and a qualified container (`Flaschen (jede 0,75 l)`).
/// The endings in `COOKIDOO_PLURAL_ENDINGS` are stripped as well, so one entry
/// covers a noun and its regular plural (`Flasche`/`Flaschen`, `lb`/`lbs`),
/// while irregular plurals (`Dosen`, `Gläser`, `słoików`) are listed outright.
///
/// Every entry was read from a live `servingSize.unitNotation` returned by
/// `<market>.tmmobile.vorwerk-digital.com` (sweep of 2026-10-14 unless the
/// comment names an earlier round of this loop; recipe ids in the comments). A
/// word MUST NOT be added here without such a sample: a wrong entry silently
/// drops the serving count of a real recipe, which is the failure this list
/// trades against.
const COOKIDOO_MEASURE_OR_CONTAINER_UNITS: &[&str] = &[
    // Mass.
    "g",   // de-DE r47931 (650 g), at r806225 (540 g)
    "mg",  // verified in an earlier round (see recipe_tests.rs)
    "kg",  // verified in an earlier round (see recipe_tests.rs)
    "gam", // vi r939387 (560 gam)
    "γρ",  // el r640582 ("γρ.")
    // Spelled-out Greek grams. The abbreviation-only entries above left
    // `γραμμάρια` (and its singular) counted as a serving count: el-GR r934408
    // carries `{"value": 200, "unitNotation": "γραμμάρια"}` and imported as
    // "Serves 200". Greek -ι/-ια is not a plural ending the form sets can
    // bridge, so both forms are listed.
    "γραμμάριο", // el-GR r934408 (singular of the live-verified "γραμμάρια")
    "γραμμάρια", // el-GR r934408 (200 γραμμάρια)
    // Greek containers and pinches from the same round's Greek sweep (33
    // Greek-language payloads). `μπουκάλια` and `φλυτζάνια` are serving units
    // (a bottle, a cup — the υ spelling of the listed `φλιτζάνια`); the pinch
    // forms were live-observed as ingredient measures.
    "μπουκάλια", // el-GR r750285 (6 μπουκάλια)
    "μπουκάλι",  // el singular of the live-verified "μπουκάλια"
    "φλυτζάνια", // el r80192 (4 φλυτζάνια)
    "φλυτζάνι",  // el singular of the live-verified "φλυτζάνια"
    "πρέζα",     // el r782201 (ingredient measure, a pinch)
    "πρέζες",    // el r818491 (ingredient measure)
    "gram",      // de r770030, gr r916841
    "gramm",     // at r981366, de r770030 ("Gramm")
    "grammi",    // it r953193, ch r76417
    "gramo",     // ca r624582
    "gramos",    // es r624650, de r1029981
    "gramas",    // ca r969343, vn r1062349
    "grames",    // be r1036420
    "grammes",   // be r1036420, us r1035410
    "gramů",     // be r143208, it r418223
    "oz",        // ca r544109 (17 oz), mx r164537
    "lb",        // verified in an earlier round (see recipe_tests.rs)
    "kilogramm", // verified in an earlier round (see recipe_tests.rs)
    // Volume.
    "ml",         // verified in an earlier round (see recipe_tests.rs)
    "cl",         // verified in an earlier round (see recipe_tests.rs)
    "dl",         // verified in an earlier round (see recipe_tests.rs)
    "l",          // verified in an earlier round (see recipe_tests.rs)
    "liter",      // verified in an earlier round (see recipe_tests.rs)
    "milliliter", // verified in an earlier round (see recipe_tests.rs)
    "litro",      // verified in an earlier round (see recipe_tests.rs)
    "cup",        // verified in an earlier round (see recipe_tests.rs)
    "cups",       // fr r420330 (6 cups)
    "杯",         // zh-Hans ca r163872 (6 杯)
    "cốc",        // vi it r666070 (5 cốc)
    "chávenas",   // pt pl r762669 (4 chávenas)
    "bardak",     // tr pt r505651 (4 bardak)
    // Spoons and pinches.
    "el",         // "EL", verified in an earlier round (see recipe_tests.rs)
    "tl",         // "TL", verified in an earlier round (see recipe_tests.rs)
    "tbsp",       // verified in an earlier round (see recipe_tests.rs)
    "tsp",        // verified in an earlier round (see recipe_tests.rs)
    "cucharada",  // es (singular of the verified "cucharadas")
    "cucharadas", // es, verified in an earlier round
    "cucchiaio",  // it, verified in an earlier round
    "cucchiai",   // it, verified in an earlier round
    "cucchiaie",  // it, verified in an earlier round
    "łyżka",      // pl, verified in an earlier round
    "łyżki",      // pl, verified in an earlier round
    "lžíce",      // cs, verified in an earlier round
    "prise",      // de "Prise"
    "prisen",     // de "Prisen"
    // Containers: bottles, jars, glasses, cups, packages.
    "flasche",       // de at r423347 ("Flaschen")
    "flaschen",      // us r779014 ("Flaschen (jede 0,75 l)")
    "schraubgläser", // ch r1001339, fr r141333
    "glas",          // de "Glas", verified in an earlier round
    "gläser",        // de r1064009, be r769015 ("Gläser")
    "glass",         // en (glass / glasses)
    "glazen",        // nl, verified in an earlier round
    "verre",         // fr be r972459 ("verres")
    "vaso",          // es mx r123375 ("vasos")
    "copo",          // pt pt r761346 ("copo"/"copos")
    "bicchiere",     // it ch r434774 ("bicchiere")
    "bicchieri",     // it es r1021775 ("bicchieri")
    "ποτήρια",       // el ch r750287
    "vasetti",       // it ca r129112
    "vasetto",       // it ch r383883
    "jar",           // en at r981728 ("jar", "jars")
    "pot",           // en, verified in an earlier round
    "tarro",         // es gr r13136 ("tarros")
    "frasco",        // pt ch r774143 ("frasco de", "frascos de")
    "bocal",         // fr vn r944754
    "bocaux",        // fr tr r77681
    "bottle",        // en ch r994881 ("bottles")
    "bottiglie",     // it tr r4771
    "garrafa",       // pt at r1084517 ("garrafa", "garrafas")
    "krukke",        // da es r911763
    "láhve",         // cs vn r6925
    "słoik",         // pl vn r5858
    "słoiki",        // pl at r10919
    "słoików",       // pl pt r970948
    "butelka",       // pl pt r798279
    "butelki",       // pl it r994886
    "bouteille",     // fr gr r991971 ("bouteilles")
    "lahve",         // cs tr r931981
    "flöskur",       // is, verified in an earlier round
    "szklanki",      // pl mx r985610
    "szklanek",      // pl ca r960220, pl r970422
    "sklenic",       // cs gb r679692
    "sklenice",      // cs be r802206 ("zavařovací sklenice")
    "zavařovací",    // cs be r802206 ("zavařovací sklenice")
    "einmachgläser", // de be r902111 ("Einmachgläser")
    "schraubglas",   // de pl r439366 ("Schraubglas")
    "tasse",         // de at r1104340 ("Tassen"), fr r381739 ("tasses")
    "tazze",         // it ca r796584
    "becher",        // de "Becher", verified in an earlier round
    "packung",       // de "Packung"/"Packungen"
    "päckchen",      // de "Päckchen"
    "gelas",         // id, verified in an earlier round
    "botol",         // id, verified in an earlier round
    "ly",            // vi ca r938802 (6 ly)
    "glös",          // is at r797841 (4 glös)
    "pahare",        // ro ca r779969 (6 pahare)
    "βαζάκια",       // el de r907097 (3 βαζάκια)
    "罐",            // zh-Hant tw r591880
    "瓶",            // zh-Hans be r626265 (8 瓶)
    "банка",         // ru be r934106 (8 банка)
    // Misc sizes.
    "cm",    // verified in an earlier round (see recipe_tests.rs)
    "克",    // zh-Hans gr r611564 (350 克)
    "dose",  // de "Dose" — Portuguese "dose" is re-accepted below
    "dosen", // de "Dosen"
    // Third sweep (2026-10-14, ~84 000 requests across the mobile API hosts):
    // measures and containers that the one-directional lookup still accepted
    // as serving counts. Each id names a payload still carrying the notation.
    "litre",     // pl-PL r1402688 (3 litres)
    "φλιτζάνια", // el-GR r808945 (2 φλιτζάνια)
    "kopper",    // el-GR r208280 (4 kopper)
    "flaske",    // nl-BE r808585 (1 flaske)
    "kavanoz",   // tr-TR r904525 (2 kavanoz)
    "botella",   // es-MX r140353 (4 botella (750 ml))
    "szklanka",  // pl-PL r933775 (1 szklanka)
    "chávena",   // de-DE r955901 (2 chávena)
    // Fourth sweep (2026-10-14, ~99 000 requests, 3 349+ payloads): measures
    // and containers still accepted, each with the id of a live payload.
    "borcane", // de-DE r154084 (2 borcane, ro)
    "taza",    // es-MX r25572 (6 tazas)
    "tazza",   // it-IT r796587 (2 tazza)
    "fincan",  // tr-TR r825712 (4 fincan)
    "litra",   // pl-PL r244089 (1 litra)
    "šálky",   // de-DE r6919 (4 šálky, cs)
    "căni",    // nl-BE r778876 (6 căni, ro)
    "чаши",    // vi-VN r930039 (4 стъклени чаши, bg)
    "гр",      // de-DE r931831 (1000 грам, гр., bg)
    "грам",    // de-DE r931831 (1000 грам, гр., bg)
    // Full sweep (2026-10-14, 141 500 requests, 5 537 payloads): measures and
    // containers still accepted, each with a live payload id.
    "βαζάκι",    // zh-TW r764166 (1 βαζάκι, el)
    "sticlă",    // fr-FR r809005 (1 sticlă, ro)
    "pohár",     // pl-PL r611882 (4 pohár, hu)
    "filiżanki", // fr-FR r959713 (4 filiżanki, pl)
    "láhev",     // pl-PL r6924 (1 láhev, cs)
    // Fifth sweep (2026-10-14: 160 011 requests across the 19 answering hosts,
    // plus focused Greek (44 000) and rare-language (30 000) sweeps):
    // containers the symmetric form-set matching still accepted, plus the
    // inflections `COOKIDOO_PLURAL_ENDINGS` cannot bridge. A form is either
    // live-observed here (id given) or the inflection of an entry the same
    // line names as live-verified; the language's own singular/plural pair was
    // checked for every entry in this list, and only the pairs the endings
    // cannot bridge are added.
    "bottiglia",   // it-IT r1005382 (1 bottiglia; also singular of "bottiglie")
    "sticle",      // ro-RO r806852 (3 sticle; also plural of "sticlă")
    "üveg",        // hu r790666 (1 befőttes üveg)
    "üvegek",      // hu plural of "üveg" (hu r790666)
    "grammo",      // it singular of "grammi" (it r953193, ch r76417)
    "gramy",       // pl/cs plural of "gram" (de r770030) and "gramů" (be r143208)
    "litry",       // pl plural of "litra" (pl-PL r244089)
    "litrów",      // pl genitive plural of "litra"
    "litr",        // pl masculine of "litra"
    "bardaklar",   // tr plural of "bardak" (tr r505651)
    "łyżek",       // pl genitive plural of "łyżka"/"łyżki"
    "ποτήρι",      // el singular of "ποτήρια" (ch r750287)
    "φλιτζάνι",    // el singular of "φλιτζάνια" (el-GR r808945)
    "lahví",       // cs genitive plural of "láhve" (vn r6925)
    "šálek",       // cs singular of "šálky" (de-DE r6919)
    "šálků",       // cs genitive plural of "šálky"
    "lžic",        // cs genitive plural of "lžíce"
    "butelek",     // pl genitive plural of "butelki"/"butelka"
    "flaska",      // is singular of "flöskur"
    "einmachglas", // de singular of "einmachgläser" (be r902111)
    "банки",       // ru plural of "банка" (be r934106)
    "банок",       // ru genitive plural of "банка"
    "kop",         // da/no singular of "kopper" (el-GR r208280)
    "kavanozlar",  // tr plural of "kavanoz" (tr-TR r904525)
    "fincanlar",   // tr plural of "fincan" (tr-TR r825712)
    "cană",        // ro singular of "căni" (nl-BE r778876)
    "чаша",        // bg singular of "чаши" (vi-VN r930039)
    "грама",       // bg plural of "грам" (de-DE r931831)
    "poharak",     // hu plural of "pohár" (pl-PL r611882)
    "filiżanka",   // pl singular of "filiżanki" (fr-FR r959713)
    "filiżanek",   // pl genitive plural of "filiżanki"
    // Containers and measures of languages the list had not covered, each
    // with the live payload that carries it (payload language in parentheses).
    "litri",   // it (de-AT r1109692, 2 litri)
    "ounce",   // fr (pt-PT r904426, 5 ounce)
    "copa",    // es (en-AU r910927, 6 copas — one entry covers the plural)
    "beker",   // nl (es-ES r75454, 1 beker)
    "potjes",  // nl (es-MX r75455, 2 potjes — also covers the singular "potje")
    "potten",  // nl (pt-PT r75453, 4 potten)
    "krukka",  // is (pl-PL r783696, 1 krukka)
    "krukkur", // is plural of "krukka"
    "koppar",  // sv (en-GB r798807, 6 koppar)
    "cangkir", // id (de-CH r781140, 6 cangkir)
    "ถ้วย",     // th (zh-TW r926293, 6 ถ้วย)
    "แก้ว",     // th (pt-PT r931435, 4 แก้ว)
    "hũ",      // vi (en-AU r668301, 1 hũ)
    "كوب",     // ar (nl-BE r944955, 6 كوب زجاجي)
    "brd",     // tr (pt-PT r260849, 4 brd — abbreviation of "bardak")
];

/// Endings Cookidoo appends to a unit in the plural. Matching intersects the
/// form sets of both sides (see `cookidoo_unit_forms`), so one entry covers a
/// noun and its regular plural in either direction (`chávena`/`chávenas`,
/// `lb`/`lbs`). Irregular plurals (`Dosen`, `Gläser`, `słoików`) are listed
/// outright.
const COOKIDOO_PLURAL_ENDINGS: &[&str] = &["en", "es", "er", "e", "s", "n"];

/// `dose`/`doses` is the Portuguese serving noun but the German container
/// `Dose`/`Dosen`, and the two share a stem. The word sits in
/// `COOKIDOO_MEASURE_OR_CONTAINER_UNITS` and is re-accepted here when the
/// payload word belongs to the same family and the payload is Portuguese; only
/// the payload `language`/`locale` can tell the homograph apart, so a missing
/// or unknown language rejects it.
const COOKIDOO_PORTUGUESE_PORTION_UNITS: &[&str] = &["dose"];

/// Whether a payload language is Portuguese. Only the `dose`/`doses` homograph
/// needs it: the German container and the Portuguese portion noun share a stem,
/// and a missing or unknown language must not re-accept the container.
fn cookidoo_is_portuguese(language: Option<&str>) -> bool {
    language
        .map(|language| language.trim().to_lowercase().starts_with("pt"))
        .unwrap_or(false)
}

/// Whether a Cookidoo `servingSize.unitNotation` denotes a serving count. A
/// missing or blank unit is treated as portions (the value is then the serving
/// count). Otherwise the unit counts as portions unless it names a measure or a
/// container — the closed class in `COOKIDOO_MEASURE_OR_CONTAINER_UNITS` — so
/// a count noun of any language (`Portionen`, `kromek`, `κομμάτια`,
/// `sneetjes`) works without being listed, while a mass, volume or container
/// unit ("650 g", "2 Gläser") does not import as "Serves 650".
///
/// `language` is the payload's `language` field, falling back to `locale`; it
/// matters only for the `dose`/`doses` homograph, which is rejected when the
/// language is missing or unknown.
fn cookidoo_serving_unit_is_portions(unit: Option<&str>, language: Option<&str>) -> bool {
    let Some(unit) = unit.map(str::trim).filter(|unit| !unit.is_empty()) else {
        return true;
    };
    let is_portuguese = cookidoo_is_portuguese(language);
    // A live `unitNotation` can list synonyms separated by '/' (es
    // "rebanada/loncha/rodaja", fr "morceau/part"); one count alternative
    // makes the value a serving count.
    unit.split('/')
        .any(|alternative| !cookidoo_unit_is_measure_or_container(alternative, is_portuguese))
}

/// Every form a unit word is looked up under: the word itself plus the word
/// with any one ending from `COOKIDOO_PLURAL_ENDINGS` removed. Matching
/// compares the form sets of the payload word and of a rejection entry, so the
/// inflected form may sit on either side: a listed plural (`chávenas`) rejects
/// a singular payload (`chávena`) just as a listed singular (`flasche`) rejects
/// a plural one (`Flaschen`). The old one-directional lookup stripped endings
/// off the payload word only, so a listed plural never matched its singular.
fn cookidoo_unit_forms(word: &str) -> impl Iterator<Item = &str> {
    std::iter::once(word).chain(
        COOKIDOO_PLURAL_ENDINGS
            .iter()
            .filter_map(|ending| word.strip_suffix(ending)),
    )
}

/// Punctuation Cookidoo attaches to a unit word in a live `unitNotation`
/// ("γρ.", "грам, гр.", "botella (750 ml)"). It is trimmed off each word before
/// the lookup so the measure or container word itself is matched.
const COOKIDOO_UNIT_PUNCTUATION: &[char] = &['.', ',', ';', '(', ')', '[', ']'];

/// Whether one '/' alternative of a Cookidoo `servingSize.unitNotation` names a
/// measure or a container (the rejection set). The alternative is lowered and
/// looked up word by word, each word stripped of surrounding punctuation, so a
/// qualified container ("Flaschen (jede 0,75 l)", "botella (750 ml)"), a
/// punctuated one ("γρ.", "грам, гр.") and a multi-word one ("zavařovací
/// sklenice") are all caught; a word matches an entry when their form sets
/// intersect (see `cookidoo_unit_forms`). A full-width parenthesis pair, as in
/// the zh-Hans "杯（玻璃杯）", is turned into a word break, so the measure word
/// inside stays visible.
fn cookidoo_unit_is_measure_or_container(alternative: &str, is_portuguese: bool) -> bool {
    let alternative = alternative.trim().to_lowercase().replace(['（', '）'], " ");
    alternative.split_whitespace().any(|word| {
        let word = word.trim_matches(COOKIDOO_UNIT_PUNCTUATION);
        if word.is_empty() {
            return false;
        }
        // "dose"/"doses" is the Portuguese portion noun but the German
        // container, so a Portuguese payload word of that family is never a
        // measure; the language scoping cannot be expressed in the form sets.
        if is_portuguese
            && cookidoo_unit_forms(word)
                .any(|form| COOKIDOO_PORTUGUESE_PORTION_UNITS.contains(&form))
        {
            return false;
        }
        COOKIDOO_MEASURE_OR_CONTAINER_UNITS.iter().any(|entry| {
            cookidoo_unit_forms(word)
                .any(|form| cookidoo_unit_forms(entry).any(|entry_form| entry_form == form))
        })
    })
}

/// Map one Cookidoo API ingredient to a meal ingredient line. Quantity and
/// unit are joined ("200 g"); preparation and the optional flag become part
/// of the name ("Parmesan (optional), in Stücken (3 cm)"). A two-bound range
/// stays in the name ("80 - 100 g Ingwer") with no quantity, as does a
/// qualitative unit word attached to a zero amount ("quelques brins de
/// coriandre fraîche", fr-FR r819463).
fn cookidoo_ingredient_line(
    item: &serde_json::Value,
    language: Option<&str>,
) -> Option<NewIngredientLine> {
    let raw_name = item.get("ingredientNotation")?.as_str()?.trim();
    if raw_name.is_empty() {
        return None;
    }

    let quantity_json = item.get("quantity");
    let unit = item
        .get("unitNotation")
        .and_then(|u| u.as_str())
        .unwrap_or("")
        .trim();

    let mut name_prefix = None;
    let quantity = match cookidoo_quantity(quantity_json) {
        Some(CookidooAmount::Amount(value)) => Some(cookidoo_amount_with_unit(value, unit)),
        Some(CookidooAmount::Range(range)) => {
            // A two-bound range is folded into the name, never the quantity:
            // downstream consumers (`plan::parse_numeric_quantity`,
            // `scaleQuantity`) read only the leading number, so "80-100 g" would
            // render as "80 -100 g" and scale to "160-100 g". Name-only matches
            // `split_ingredient_line`, which also keeps range lines whole.
            name_prefix = Some(cookidoo_amount_with_unit(range, unit));
            None
        }
        Some(CookidooAmount::Zero) => {
            // `{"value": 0}` is Cookidoo's "no amount given" (de-AT r1017650
            // "sal"), so the amount is dropped. A *qualitative* unit word
            // (fr-FR r819463 "quelques", it-IT r250249 "适量", el-GR r749573
            // "λίγο") stays visible in the name — the public page keeps it next
            // to the ingredient and renders no amount — while a measure or
            // container unit ("0 g") disappears with the amount.
            if !unit.is_empty()
                && !cookidoo_unit_is_measure_or_container(unit, cookidoo_is_portuguese(language))
            {
                name_prefix = Some(unit.to_string());
            }
            None
        }
        None => None,
    };

    let mut name = String::new();
    if let Some(prefix) = name_prefix {
        name.push_str(&prefix);
        name.push(' ');
    }
    name.push_str(raw_name);
    if item
        .get("optional")
        .and_then(|o| o.as_bool())
        .unwrap_or(false)
    {
        name.push_str(" (optional)");
    }
    let preparation = item
        .get("preparation")
        .and_then(|p| p.as_str())
        .map(|p| p.trim().trim_start_matches(',').trim())
        .filter(|p| !p.is_empty());
    if let Some(preparation) = preparation {
        name.push_str(", ");
        name.push_str(preparation);
    }

    Some(NewIngredientLine {
        name: truncate(&name, COOKIDOO_NAME_LIMIT),
        quantity: quantity.as_deref().map(|q| truncate(q, 50)),
    })
}

/// Byte limit for Cookidoo ingredient names, matching `db::validate_meal`.
const COOKIDOO_NAME_LIMIT: usize = 100;

/// Byte budget for the group title appended when disambiguating a duplicate
/// ingredient name; keeps the suffix well inside `COOKIDOO_NAME_LIMIT`.
const COOKIDOO_GROUP_TITLE_LIMIT: usize = 60;

/// Give a Cookidoo ingredient line a name no earlier line in the same draft
/// claims. `db::set_meal_ingredients` inserts one `meal_ingredients` row per
/// line under `PRIMARY KEY (meal_id, ingredient_id)`, so two lines resolving to
/// the same ingredient — Cookidoo repeats "Milch" in "Hefeteig" and in
/// "Füllung und Fertigstellung", de-DE r51900 — raise a uniqueness error, roll
/// the transaction back and fail the whole import with HTTP 500. The later line
/// keeps its own amount and is distinguished by its group title, falling back
/// to a numeric suffix when the title is missing or already taken.
fn disambiguate_cookidoo_ingredient(
    mut line: NewIngredientLine,
    group_title: &str,
    seen: &mut HashSet<String>,
) -> NewIngredientLine {
    if seen.insert(cookidoo_ingredient_name_key(&line.name)) {
        return line;
    }

    let base = std::mem::take(&mut line.name);
    let group_suffix = (!group_title.is_empty())
        .then(|| format!(" ({})", truncate(group_title, COOKIDOO_GROUP_TITLE_LIMIT)));
    let numeric_suffixes = (2..).map(|n| format!(" ({n})"));
    for suffix in group_suffix.into_iter().chain(numeric_suffixes) {
        let candidate = cookidoo_suffixed_name(&base, &suffix);
        if seen.insert(cookidoo_ingredient_name_key(&candidate)) {
            line.name = candidate;
            return line;
        }
    }
    unreachable!("a numeric suffix always yields a free name")
}

/// Collision key for two Cookidoo lines. `db::set_meal_ingredients` stores
/// `db::normalize_ingredient_name` output and `upsert_ingredients` looks it up
/// with `COLLATE NOCASE`, so names differing only in whitespace or case land on
/// the same `ingredients` row — and therefore on the same `meal_ingredients`
/// primary key.
fn cookidoo_ingredient_name_key(name: &str) -> String {
    crate::db::normalize_ingredient_name(name).to_lowercase()
}

/// Compose `base` and `suffix` within `COOKIDOO_NAME_LIMIT`. The suffix's bytes
/// are reserved so truncation can never cut it off and revive the collision.
fn cookidoo_suffixed_name(base: &str, suffix: &str) -> String {
    let base_limit = COOKIDOO_NAME_LIMIT.saturating_sub(suffix.len());
    format!("{}{suffix}", truncate(base, base_limit))
}

/// Resolved amount of a Cookidoo quantity object.
enum CookidooAmount {
    /// Scalar `value` or a single `from`/`to` bound, as display text.
    Amount(String),
    /// Two-bound range rendered "80 - 100".
    Range(String),
    /// An explicit zero (`{"value": 0}`): Cookidoo's "no amount given".
    Zero,
}

/// Join an amount and its unit ("80 - 100" + "g" -> "80 - 100 g").
fn cookidoo_amount_with_unit(amount: String, unit: &str) -> String {
    match unit.is_empty() {
        true => amount,
        false => format!("{amount} {unit}"),
    }
}

/// Whether an amount field is a numeric zero. Cookidoo writes `{"value": 0}`
/// for "no amount given" — 6 181 of the 23 945 payloads of the 2026-10-14
/// sweep carry it, for 9 038 ingredient lines — so zero must not be rendered as
/// an amount. The value is a JSON number in every sampled payload; a numeric
/// string is accepted too.
fn cookidoo_amount_is_zero(value: &serde_json::Value) -> bool {
    match value {
        serde_json::Value::Number(n) => n.as_f64() == Some(0.0),
        serde_json::Value::String(s) => s.trim().parse::<f64>() == Ok(0.0),
        _ => false,
    }
}

/// Resolve a Cookidoo quantity object. Scalar amounts carry `value`; range
/// amounts carry `from`/`to` instead, so a single bound still yields an amount
/// while both bounds yield a range. An explicit zero is `Zero`, not an amount:
/// a zero bound on its own (never seen live) yields no amount, and only the
/// ingredient line knows whether a unit word should survive it.
fn cookidoo_quantity(quantity: Option<&serde_json::Value>) -> Option<CookidooAmount> {
    let quantity = quantity?;
    if let Some(value) = quantity.get("value") {
        if cookidoo_amount_is_zero(value) {
            return Some(CookidooAmount::Zero);
        }
        let value = cookidoo_amount_text(value);
        if !value.is_empty() {
            return Some(CookidooAmount::Amount(value));
        }
    }
    let from = quantity
        .get("from")
        .map(cookidoo_amount_text)
        .unwrap_or_default();
    let to = quantity
        .get("to")
        .map(cookidoo_amount_text)
        .unwrap_or_default();
    match (from.is_empty(), to.is_empty()) {
        (false, false) => Some(CookidooAmount::Range(format!("{from} - {to}"))),
        (false, true) => Some(CookidooAmount::Amount(from)),
        (true, false) => Some(CookidooAmount::Amount(to)),
        (true, true) => None,
    }
}

/// Render one Cookidoo amount field (number or numeric string) as text. A zero
/// renders empty, so a `from`/`to` bound of zero yields no amount either.
fn cookidoo_amount_text(value: &serde_json::Value) -> String {
    if cookidoo_amount_is_zero(value) {
        return String::new();
    }
    match value {
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::String(s) => s.trim().to_string(),
        _ => String::new(),
    }
}

/// Extract the best available image URL from a Cookidoo API payload. Asset
/// URLs contain a `{transformation}` placeholder and have no file extension;
/// both are filled in to produce a loadable JPEG URL.
pub(crate) fn cookidoo_image_url(json: &serde_json::Value) -> Option<String> {
    let asset = json.get("descriptiveAssets")?.as_array()?.first()?;
    let url = asset
        .get("landscape")
        .or_else(|| asset.get("square"))
        .or_else(|| asset.get("portrait"))
        .and_then(|u| u.as_str())?;
    let url = url.replace("{transformation}", "t_web_rdp_recipe_584x480_1_5x");
    if url.rsplit('/').next().unwrap_or("").contains('.') {
        Some(url)
    } else {
        Some(format!("{url}.jpg"))
    }
}

/// Parse recipe and return the draft plus the raw image URL (if found in JSON-LD).
/// `fetch_and_parse` uses the image URL to download; `parse_recipe` discards it.
fn parse_recipe_with_image_url(text: &str) -> Result<(ImportDraft, Option<String>), AppError> {
    // Use scraper to find all JSON-LD script blocks
    let document = scraper::Html::parse_document(text);
    let selector =
        scraper::Selector::parse(r#"script[type="application/ld+json"]"#).expect("static selector");

    // Collect (raw_json_value, schema_entry) pairs for blocks that parse successfully
    let mut pairs: Vec<(serde_json::Value, SchemaOrgEntry)> = Vec::new();
    // Raw JSON of every block that parses as JSON. Fallback parse below runs
    // only when the `recipe-scraper` path finds no recipe at all, so keeping
    // all blocks is safe: a block can deserialize yet yield zero recipes
    // (e.g. a WPRM `Recipe` node with mixed `HowToStep`/`HowToSection`
    // instructions falls into the catch-all `Nonsense` variant via its `@id`).
    let mut raw_blocks: Vec<serde_json::Value> = Vec::new();

    for element in document.select(&selector) {
        let block_text = element.text().collect::<String>();
        // Try serde_json parse for image extraction
        if let Ok(json_value) = serde_json::from_str::<serde_json::Value>(&block_text) {
            // Try recipe_scraper parse for recipe extraction
            if let Ok(entry) = SchemaOrgEntry::from_json_str(&block_text) {
                pairs.push((json_value.clone(), entry));
            }
            raw_blocks.push(json_value);
        }
    }

    // Fallback: if no script blocks found, try parsing the text directly as raw JSON-LD
    if pairs.is_empty()
        && raw_blocks.is_empty()
        && let Ok(json_value) = serde_json::from_str::<serde_json::Value>(text)
    {
        if let Ok(entry) = SchemaOrgEntry::from_json_str(text) {
            pairs.push((json_value.clone(), entry));
        }
        raw_blocks.push(json_value);
    }

    // Extract the first Recipe from all SchemaEntry objects
    for (json_value, entry) in &pairs {
        let recipes: Vec<_> = entry.extract_recipes();
        if let Some(recipe) = recipes.into_iter().next() {
            let name = recipe.name().to_string();
            let ingredients = recipe
                .ingredients()
                .clone()
                .into_iter()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.is_empty() {
                        None
                    } else {
                        Some(split_ingredient_line(trimmed))
                    }
                })
                .collect();
            let raw_instructions = recipe
                .directions()
                .as_ref()
                .map_or_else(String::new, |list| {
                    if let Some(dirs) = list.directions() {
                        dirs.iter()
                            .map(|d| d.to_string())
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else if let Some(sections) = list.sections() {
                        sections
                            .cloned()
                            .flat_map(|s| s.into_iter().map(|d| d.to_string()).collect::<Vec<_>>())
                            .collect::<Vec<_>>()
                            .join("\n")
                    } else {
                        String::new()
                    }
                });
            let instructions = decode_html_entities(&sanitize_instructions(&raw_instructions));

            let image_url =
                extract_image_url(json_value).or_else(|| find_sibling_image_url(json_value));

            let portions = recipe.yields().as_ref().and_then(|y| {
                let s = y.to_string();
                let num_str: String = s
                    .chars()
                    .skip_while(|c| !c.is_ascii_digit())
                    .take_while(|c| c.is_ascii_digit())
                    .collect();
                num_str.parse::<i32>().ok()
            });

            return Ok((
                ImportDraft {
                    name,
                    ingredients,
                    instructions,
                    image_base64: None,
                    portions,
                    source_url: None,
                },
                image_url,
            ));
        }
    }

    // Fallback for blocks `recipe-scraper` rejects (e.g. WPRM output mixing
    // `HowToStep` entries with a trailing `HowToSection`): parse the raw
    // JSON manually and flatten the instructions.
    for block in &raw_blocks {
        let mut nodes = Vec::new();
        collect_recipe_nodes(block, &mut nodes);
        for node in nodes {
            if let Some((draft, image_url)) = draft_from_recipe_json(node) {
                let image_url = image_url.or_else(|| find_sibling_image_url(block));
                return Ok((draft, image_url));
            }
        }
    }

    Err(AppError::Import(
        "no machine-readable recipe found on this page".into(),
        "import_no_recipe",
    ))
}

/// Collect `Recipe`-typed nodes from a raw JSON-LD block (handles `@graph`
/// objects, bare `Recipe` objects, and arrays of either).
fn collect_recipe_nodes<'a>(json: &'a serde_json::Value, out: &mut Vec<&'a serde_json::Value>) {
    if let Some(graph) = json.get("@graph").and_then(|g| g.as_array()) {
        out.extend(graph.iter().filter(|item| is_recipe_type(item)));
    } else if let Some(arr) = json.as_array() {
        for item in arr {
            collect_recipe_nodes(item, out);
        }
    } else if is_recipe_type(json) {
        out.push(json);
    }
}

/// Build a draft from a raw `Recipe` JSON object without `recipe-scraper`.
/// Returns `None` when the node has no usable name (not a recipe draft).
fn draft_from_recipe_json(json: &serde_json::Value) -> Option<(ImportDraft, Option<String>)> {
    let name = json.get("name")?.as_str()?.trim().to_string();
    if name.is_empty() {
        return None;
    }

    let ingredients = match json.get("recipeIngredient") {
        Some(serde_json::Value::String(s)) => vec![s.as_str()],
        Some(serde_json::Value::Array(arr)) => {
            arr.iter().filter_map(|v| v.as_str()).collect::<Vec<_>>()
        }
        Some(serde_json::Value::Object(obj)) => {
            // Some sites (e.g. sallys-blog) emit a JSON object with
            // numeric-string keys instead of an array. Sort numeric-aware
            // so "10" orders after "9"; non-numeric keys sort last.
            let mut entries: Vec<(Option<u64>, &str, &str)> = obj
                .iter()
                .filter_map(|(k, v)| v.as_str().map(|s| (k.parse::<u64>().ok(), k.as_str(), s)))
                .collect();
            entries.sort_by(|a, b| match (&a.0, &b.0) {
                (Some(x), Some(y)) => x.cmp(y).then_with(|| a.1.cmp(b.1)),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => a.1.cmp(b.1),
            });
            entries.into_iter().map(|(_, _, s)| s).collect::<Vec<_>>()
        }
        _ => Vec::new(),
    }
    .into_iter()
    .filter_map(|line| {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            None
        } else {
            Some(split_ingredient_line(trimmed))
        }
    })
    .collect();

    let mut steps = Vec::new();
    if let Some(instructions) = json.get("recipeInstructions") {
        flatten_instruction_texts(instructions, &mut steps);
    }
    let instructions = decode_html_entities(&sanitize_instructions(&steps.join("\n")));

    let portions = parse_yield_portions(json.get("recipeYield"));
    let image_url = extract_image_url(json);

    Some((
        ImportDraft {
            name,
            ingredients,
            instructions,
            image_base64: None,
            portions,
            source_url: None,
        },
        image_url,
    ))
}

/// Recursively flatten `recipeInstructions` into step texts. Handles plain
/// strings, `HowToStep` (`text`), `HowToSection` (`itemListElement`), arrays
/// mixing both, and generic wrapper objects.
fn flatten_instruction_texts(json: &serde_json::Value, out: &mut Vec<String>) {
    match json {
        serde_json::Value::String(s) => {
            let trimmed = s.trim();
            if !trimmed.is_empty() {
                out.push(trimmed.to_string());
            }
        }
        serde_json::Value::Array(arr) => {
            for item in arr {
                flatten_instruction_texts(item, out);
            }
        }
        serde_json::Value::Object(obj) => {
            if let Some(nested) = obj.get("itemListElement") {
                flatten_instruction_texts(nested, out);
            } else if let Some(text) = obj.get("text") {
                flatten_instruction_texts(text, out);
            }
        }
        _ => {}
    }
}

/// Extract the serving count from a `recipeYield` value (string, number, or
/// array — first element wins), mirroring the `recipe-scraper` path.
fn parse_yield_portions(yield_value: Option<&serde_json::Value>) -> Option<i32> {
    fn yield_to_str(v: &serde_json::Value) -> Option<String> {
        match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => Some(n.to_string()),
            _ => None,
        }
    }
    let s = match yield_value? {
        serde_json::Value::Array(arr) => arr.first().and_then(yield_to_str)?,
        other => yield_to_str(other)?,
    };
    let num_str: String = s
        .chars()
        .skip_while(|c| !c.is_ascii_digit())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    num_str.parse::<i32>().ok()
}

/// Unit words recognized as a quantity prefix in ingredient lines.
/// Language-specific: currently English + German (see `*_german_*` /
/// `*_bare_count_*` cases in `recipe_tests.rs`). A new import language MUST
/// add its unit words here, or its quantities silently land in the
/// ingredient name (quantity `None`).
const UNITS: &[&str] = &[
    "el",
    "esslöffel",
    "tl",
    "teelöffel",
    "prise",
    "prisen",
    "päckchen",
    "pck",
    "bund",
    "zehe",
    "zehen",
    "scheibe",
    "scheiben",
    "stück",
    "stueck",
    "dose",
    "dosen",
    "glas",
    "gläser",
    "becher",
    "tasse",
    "tassen",
    "cup",
    "cups",
    "tbsp",
    "tablespoon",
    "tablespoons",
    "tsp",
    "teaspoon",
    "teaspoons",
    "g",
    "gram",
    "grams",
    "gramm",
    "kg",
    "kilogram",
    "kilograms",
    "kilogramm",
    "ml",
    "milliliter",
    "milliliters",
    "dl",
    "cl",
    "l",
    "liter",
    "liters",
    "oz",
    "ounce",
    "ounces",
    "lb",
    "lbs",
    "pound",
    "pounds",
    "clove",
    "cloves",
    "slice",
    "slices",
    "piece",
    "pieces",
    "pinch",
    "dash",
    "quart",
    "quarts",
    "pint",
    "pints",
    "gallon",
    "gallons",
    "stick",
    "sticks",
    "bunch",
    "bunches",
    "handful",
    "handfuls",
    "can",
    "cans",
];

/// Decode HTML entities in a recipe string (`&frac12;`, `&nbsp;`, `&amp;`,
/// decimal/hex numeric references). Unknown entities pass through unchanged.
pub(crate) fn decode_html_entities(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(amp) = rest.find('&') {
        out.push_str(&rest[..amp]);
        rest = &rest[amp..];
        let Some(semi) = rest.find(';') else {
            out.push_str(rest);
            break;
        };
        if semi > 32 {
            // Not a plausible entity; copy the `&` and keep scanning.
            out.push('&');
            rest = &rest[1..];
            continue;
        }
        let entity = &rest[1..semi];
        let decoded = if entity.starts_with("#x") || entity.starts_with("#X") {
            u32::from_str_radix(&entity[2..], 16)
                .ok()
                .and_then(char::from_u32)
        } else if let Some(num) = entity.strip_prefix('#') {
            num.parse::<u32>().ok().and_then(char::from_u32)
        } else {
            match entity {
                "nbsp" => Some(' '),
                "amp" => Some('&'),
                "lt" => Some('<'),
                "gt" => Some('>'),
                "quot" => Some('"'),
                "apos" => Some('\''),
                "frac12" => Some('½'),
                "frac14" => Some('¼'),
                "frac34" => Some('¾'),
                _ => None,
            }
        };
        match decoded {
            Some(c) => out.push(c),
            None => out.push_str(&rest[..=semi]),
        }
        rest = &rest[semi + 1..];
    }
    out.push_str(rest);
    out
}

/// Split an ingredient line into name and optional quantity.
/// Best-effort: if the line starts with a number, the leading number (plus a
/// following unit word, if any) is the quantity and the rest is the name.
/// Otherwise the whole line is the name.
pub(crate) fn split_ingredient_line(line: &str) -> NewIngredientLine {
    let decoded = decode_html_entities(line);
    let line = decoded.as_str();
    let tokens: Vec<&str> = line.split_whitespace().collect();
    if tokens.is_empty() {
        return NewIngredientLine {
            name: truncate(line.trim(), 100),
            quantity: None,
        };
    }
    // Leading amount ("2", "1/2", "1,5", "400g", "½") — possibly with a unit
    // glued on ("400g", "2EL"). Mixed tokens like "7-Kräuter" are names.
    let Some((amount, glued_unit)) = split_leading_amount(tokens[0]) else {
        return NewIngredientLine {
            name: truncate(line.trim(), 100),
            quantity: None,
        };
    };
    if tokens.len() < 2 {
        return NewIngredientLine {
            name: truncate(line.trim(), 100),
            quantity: None,
        };
    }

    // Glued unit ("400g Mehl", "2EL Öl"): amount + glued suffix is the quantity.
    // Unknown suffix ("400xyz Mehl") stays name-only instead of dropping text.
    if let Some(unit) = glued_unit {
        let unit_lower = unit.to_lowercase();
        if UNITS.contains(&unit_lower.as_str()) {
            let name = tokens[1..].join(" ");
            if !name.is_empty() {
                return NewIngredientLine {
                    name: truncate(name.trim(), 100),
                    quantity: Some(truncate(format!("{amount} {unit}").trim(), 50)),
                };
            }
        }
        return NewIngredientLine {
            name: truncate(line.trim(), 100),
            quantity: None,
        };
    }

    // Check if the second token (or sometimes third) is a unit word
    let unit_idx = tokens.iter().skip(1).take(2).position(|t| {
        UNITS.contains(&t.to_lowercase().trim_end_matches(',').trim_end_matches('.'))
    });

    if let Some(rel_idx) = unit_idx {
        let unit_end = 1 + rel_idx + 1; // number + unit
        let quantity = tokens[..unit_end].join(" ");
        let name = tokens[unit_end..].join(" ");
        if !name.is_empty() {
            return NewIngredientLine {
                name: truncate(name.trim(), 100),
                quantity: Some(truncate(quantity.trim(), 50)),
            };
        }
    }

    // Bare count without a unit word ("2 rote Paprika", "1 Zwiebel"):
    // the leading number is the quantity, the rest is the name. A second
    // numeric token ("1 - 2 Zwiebeln", "1 x 400g") is a range/pack size,
    // not a count — keep the whole line name-only like before.
    let second = tokens[1].trim_end_matches(',').trim_end_matches('.');
    if second == "-"
        || second == "–"
        || second == "—"
        || second == "x"
        || second == "×"
        || split_leading_amount(second).is_some()
    {
        return NewIngredientLine {
            name: truncate(line.trim(), 100),
            quantity: None,
        };
    }
    let name = tokens[1..].join(" ");
    if !name.is_empty() {
        return NewIngredientLine {
            name: truncate(name.trim(), 100),
            quantity: Some(truncate(amount.trim(), 50)),
        };
    }

    NewIngredientLine {
        name: truncate(line.trim(), 100),
        quantity: None,
    }
}

/// Split a leading token into its numeric amount and an optional glued-on unit
/// suffix (`"400g"` → `("400", "g")`, `"2"` → `("2", None)`).
/// Returns `None` when the token is not a pure amount (e.g. `"7-Kräuter"` or
/// `"etwas"`), so such lines stay name-only.
fn split_leading_amount(token: &str) -> Option<(&str, Option<&str>)> {
    const FRACTIONS: &[char] = &['½', '⅓', '⅔', '¼', '¾', '⅛', '⅜', '⅝', '⅞'];
    let cut = token
        .char_indices()
        .take_while(|(_, c)| {
            c.is_ascii_digit() || *c == '/' || *c == '.' || *c == ',' || FRACTIONS.contains(c)
        })
        .map(|(i, c)| i + c.len_utf8())
        .last()?;
    if cut == 0 {
        return None;
    }
    let (amount, mut rest) = token.split_at(cut);
    // A trailing separator ("1.", "1,") is ordinal/punctuation, not an amount.
    if amount.ends_with(['.', ',']) {
        return None;
    }
    // A bare separator ("/", ".", ",") is not an amount.
    if !amount
        .chars()
        .any(|c| c.is_ascii_digit() || FRACTIONS.contains(&c))
    {
        return None;
    }
    // Trailing punctuation on a glued suffix ("400g,") belongs to the line,
    // not the unit.
    rest = rest.trim_end_matches(['.', ',']);
    if rest.is_empty() {
        return Some((amount, None));
    }
    if rest.chars().all(|c| c.is_alphabetic()) {
        return Some((amount, Some(rest)));
    }
    None
}

/// Truncate a string to at most `max` bytes, appending `…` when the input is
/// longer. The ellipsis is counted inside the byte budget, and the cut is
/// floored to a UTF-8 char boundary so multibyte names cannot panic and the
/// result never exceeds `max` bytes (downstream validation counts bytes).
pub fn truncate(s: &str, max: usize) -> String {
    const ELLIPSIS: &str = "…";
    if s.len() <= max {
        return s.to_string();
    }
    // No room for the ellipsis (max < 3): cut without it rather than exceed `max`.
    let suffix = if max >= ELLIPSIS.len() { ELLIPSIS } else { "" };
    let mut end = max - suffix.len();
    while !s.is_char_boundary(end) {
        end -= 1;
    }
    format!("{}{suffix}", &s[..end])
}

/// Extract the image URL from a raw JSON-LD value.
/// The `image` field can be: a URL string, an array of URL strings,
/// a single `ImageObject` with a `url` field, or an array of `ImageObject`s.
pub(crate) fn extract_image_url(json: &serde_json::Value) -> Option<String> {
    // If @graph array, find the first element with @type containing "Recipe"
    if let Some(graph) = json.get("@graph").and_then(|g| g.as_array()) {
        for item in graph {
            if is_recipe_type(item)
                && let Some(url) = extract_image_url(item)
            {
                return Some(url);
            }
        }
    }

    // Top-level arrays (e.g. `[Recipe, BreadcrumbList]`): recurse into items.
    if let Some(arr) = json.as_array() {
        for item in arr {
            if let Some(url) = extract_image_url(item) {
                return Some(url);
            }
        }
    }

    let img = json.get("image")?;
    match img {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => arr.first().and_then(|v| match v {
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Object(o) => o.get("url").and_then(|u| u.as_str()).map(String::from),
            _ => None,
        }),
        serde_json::Value::Object(o) => o
            .get("url")
            .or_else(|| o.get("contentUrl"))
            .and_then(|u| u.as_str())
            .map(String::from),
        _ => None,
    }
}

/// Scan a JSON-LD block's sibling nodes for an `ImageObject` URL (e.g. a
/// chefkoch `@graph` whose Recipe `image` is an `@id`-only reference).
fn find_sibling_image_url(json: &serde_json::Value) -> Option<String> {
    fn node_url(node: &serde_json::Value) -> Option<String> {
        let is_image_object = match node.get("@type") {
            Some(serde_json::Value::String(s)) => s == "ImageObject",
            Some(serde_json::Value::Array(arr)) => {
                arr.iter().any(|t| t.as_str() == Some("ImageObject"))
            }
            _ => false,
        };
        if !is_image_object {
            return None;
        }
        node.get("url")
            .or_else(|| node.get("contentUrl"))
            .and_then(|u| u.as_str())
            .map(String::from)
    }
    if let Some(graph) = json.get("@graph").and_then(|g| g.as_array()) {
        graph.iter().find_map(node_url)
    } else if let Some(arr) = json.as_array() {
        arr.iter().find_map(node_url)
    } else {
        node_url(json)
    }
}

/// Check if a JSON-LD value has `@type` containing "Recipe".
fn is_recipe_type(json: &serde_json::Value) -> bool {
    match json.get("@type") {
        Some(serde_json::Value::String(s)) => s == "Recipe",
        Some(serde_json::Value::Array(arr)) => arr.iter().any(|t| t.as_str() == Some("Recipe")),
        _ => false,
    }
}

/// Download an image URL and convert to JPEG bytes. Best-effort: returns None on any failure.
pub(crate) async fn try_download_image(client: &reqwest::Client, url: &str) -> Option<Vec<u8>> {
    let resp = client.get(url).send().await.ok()?;
    if !resp.status().is_success() {
        return None;
    }
    let bytes = resp.bytes().await.ok()?;
    let jpeg = crate::image::convert_to_jpeg(&bytes).ok()?;
    Some(jpeg)
}

/// Download an image URL and convert to JPEG bytes via the standard pipeline
/// (`convert_to_jpeg`, q82, 3840px max long edge). Returns a structured error
/// on any failure so callers can surface actionable messages to the user.
pub(crate) async fn download_image_from_url(url: &str) -> Result<Vec<u8>, AppError> {
    let parsed_url =
        reqwest::Url::parse(url).map_err(|_| AppError::BadRequest("invalid image URL".into()))?;

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(30))
        .connect_timeout(Duration::from_secs(10))
        .user_agent("Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/131.0.0.0 Safari/537.36")
        .build()
        .map_err(|e| AppError::Internal(format!("failed to build HTTP client: {e}")))?;

    let resp = client
        .get(parsed_url)
        .send()
        .await
        .map_err(|e| AppError::BadRequest(format!("image URL unreachable: {e}")))?;

    if !resp.status().is_success() {
        return Err(AppError::BadRequest(format!(
            "image URL returned HTTP {}",
            resp.status()
        )));
    }

    let bytes = resp
        .bytes()
        .await
        .map_err(|e| AppError::BadRequest(format!("failed to download image: {e}")))?;

    let jpeg = crate::image::convert_to_jpeg(&bytes)?;
    Ok(jpeg)
}

/// Sanitize HTML in imported instructions to a safe whitelist.
/// Allows only: p, br, strong, em, b, i, ul, ol, li. Strips all attributes.
/// Drops the *content* of script/style tags. Plain text passes through.
/// Returns "" if the result is empty/whitespace-only.
pub fn sanitize_instructions(html: &str) -> String {
    let tags: HashSet<&str> =
        HashSet::from(["p", "br", "strong", "em", "b", "i", "ul", "ol", "li"]);
    let clean_content: HashSet<&str> = HashSet::from(["script", "style"]);
    let sanitized = ammonia::Builder::empty()
        .add_tags(&tags)
        .clean_content_tags(clean_content)
        .clean(html)
        .to_string();
    if sanitized.trim().is_empty() {
        String::new()
    } else {
        sanitized
    }
}

/// Strip all HTML tags and return the plain text content with whitespace
/// collapsed to single spaces. Uses `ammonia` with no allowed tags, plus
/// `clean_content_tags` to drop script/style/noscript/nav/footer/header
/// elements entirely.
pub fn extract_readable_text(html: &str) -> String {
    let clean_content: HashSet<&str> =
        HashSet::from(["script", "style", "noscript", "nav", "footer", "header"]);
    ammonia::Builder::empty()
        .clean_content_tags(clean_content)
        .clean(html)
        .to_string()
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// Resolve a possibly-relative URL against a base URL. Returns None if parsing fails.
fn resolve_url(url: &str, base_url: &str) -> Option<String> {
    let base = reqwest::Url::parse(base_url).ok()?;
    reqwest::Url::options()
        .base_url(Some(&base))
        .parse(url.trim())
        .ok()
        .map(|u| u.to_string())
}

/// Extract candidate image URLs from raw HTML.
/// Checks, in priority order: OpenGraph `og:image`, JSON-LD `image`,
/// and `<img>` tags with recipe-relevant classes. Returns de-duplicated, absolute URLs.
/// Returns an empty Vec (not an error) if no image URLs are found.
pub fn extract_image_urls_from_html(html: &str, base_url: &str) -> Vec<String> {
    let document = scraper::Html::parse_document(html);
    let mut urls: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();

    // 1. OpenGraph og:image
    if let Ok(sel) = scraper::Selector::parse(r#"meta[property="og:image"]"#) {
        for el in document.select(&sel) {
            if let Some(content) = el.value().attr("content")
                && let Some(abs) = resolve_url(content, base_url)
                && seen.insert(abs.clone())
            {
                urls.push(abs);
            }
        }
    }

    // 2. JSON-LD image (reuse existing extract_image_url)
    if let Ok(sel) = scraper::Selector::parse(r#"script[type="application/ld+json"]"#) {
        for el in document.select(&sel) {
            let block = el.text().collect::<String>();
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(&block)
                && let Some(img) =
                    extract_image_url(&json).or_else(|| find_sibling_image_url(&json))
                && let Some(abs) = resolve_url(&img, base_url)
                && seen.insert(abs.clone())
            {
                urls.push(abs);
            }
        }
    }

    // 3. <img> tags with recipe-relevant classes
    if let Ok(sel) = scraper::Selector::parse("img") {
        for el in document.select(&sel) {
            if let Some(src) = el.value().attr("src") {
                let class = el.value().attr("class").unwrap_or("");
                let is_relevant = class.split_whitespace().any(|c| {
                    matches!(
                        c,
                        "wp-post-image"
                            | "attachment-post-thumbnail"
                            | "size-post-thumbnail"
                            | "recipe-image"
                            | "featured-image"
                    )
                });
                if is_relevant
                    && let Some(abs) = resolve_url(src, base_url)
                    && seen.insert(abs.clone())
                {
                    urls.push(abs);
                }
            }
        }
    }

    urls
}

/// Returns `true` when `s` is a bare `http://` or `https://` URL with no
/// surrounding whitespace — i.e. the entire trimmed string is a single URL
/// token.
pub fn is_bare_url(s: &str) -> bool {
    let s = s.trim();
    if s.is_empty() || s.contains(char::is_whitespace) {
        return false;
    }
    match reqwest::Url::parse(s) {
        Ok(u) => u.scheme() == "http" || u.scheme() == "https",
        Err(_) => false,
    }
}
