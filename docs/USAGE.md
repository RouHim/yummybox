# Using YummyBox

YummyBox opens on the current week's plan. If that week has no plan yet, it opens the planner.

## Meals

The meals list is at `/meals` and shows recently updated meals first. The search field filters by meal name and ingredient name.

Add meal opens a dialog with four tabs: Manual, Import URLs, AI import, and ZIP file. In the manual form, each ingredient goes on its own line with an optional quantity:

```text
200g pasta
2 eggs
salt to taste
```

Quantity text is kept as written. The planner adds numeric quantities it can parse and shows a unit only when every line uses the same one.

Select a card to open the detail view. The card actions and the detail view can edit or delete a meal. Servings accept values from 1 to 10000.

## Cooking view

Open a meal at `/meals/{id}`. Instructions stored as HTML are sanitized and shown as formatted paragraphs; plain text keeps its line breaks.

When a meal has servings, the view shows Serves N. The stepper scales every ingredient quantity that has a number. Returning to the stored count restores the original amounts.

Polish instructions asks the configured model to rewrite the steps and saves the result. It needs AI configuration.

## Meal images

Add a photo by pasting from the clipboard, entering an image URL, or dropping a file on the image area. Non-image files are rejected with an inline error.

Stored images are JPEG. An image whose long edge exceeds 3840 px is downscaled first. Meals without an image show a generic card icon.

## Planner

The planner is at `/planner`. Pick a week, choose 1 to 20 meals, and generate a plan. Meals planned less recently are more likely to be selected.

Use the meal picker to add or remove meals. Delete the plan to generate a new one.

The ingredient summary merges repeated ingredient names and adds the numeric quantities it can parse. A unit is shown only when every line for that ingredient uses the same one.

Each summary item has a Bring! button. It sends the item to the first Bring! shopping list in the account.

## Spontaneous cooking

Open `/spontaneous` and list the ingredients you have, one per line. You can add up to 500 characters of extra instructions and up to five photos, with each photo up to 20 MB.

A configured model drafts a recipe from the ingredients, the hint, and the photos. Vision input needs a vision-capable model. Edit the draft, save it to the collection, or select Cook now to open it in the cooking view without saving.

The draft survives a page reload. Start over discards it.

## Recipe import

The import tabs live in the add meal dialog.

### URLs

Paste one recipe URL per line, up to 50 lines. Each line must start with `http://` or `https://`.

YummyBox reads the page's JSON-LD recipe data. A site that publishes the recipe only as visible text cannot be imported; the AI tab or the manual form is the fallback.

Cookidoo links are read through the Cookidoo mobile API first, because its public pages hide the preparation steps. If that call fails, YummyBox parses the page markup.

A single successful URL opens the new meal. Several successes reload the list.

### AI import

Choose a provider and model, then add a text hint, up to five photos, or both. The extracted recipe opens in the manual form for review before saving. Photos need a vision-capable model.

### ZIP and Mealie

The ZIP tab imports a YummyBox export or a Mealie backup `.zip`. Recipes with a name that already exists are skipped. The result reports how many were imported, skipped, or failed.

### Export

The meals list menu has Export all. It downloads a ZIP with `recipes.json` and the meal images. Import it through the ZIP tab on another install.

## Settings and AI

Open `/settings` with the gear in the top bar.

The AI section stores the provider, model, API key, and custom base URL. The model list is fetched from the provider. A provider that needs a key and has none is marked as not configured.

The Bring! section stores the account email and password and shows whether the connection works.

Stored values take precedence over environment variables. A stored API key or Bring! password is never sent back to the browser; the page shows only whether it is set and whether it comes from settings or the environment. Clearing a stored value falls back to the environment variable.

A stored key belongs to the provider and endpoint it was saved for. Changing the provider or the custom base URL without entering a new key clears it. Changing the provider without choosing a model clears the model. A key cannot be saved before a provider exists, and a custom key needs the base URL first.

Ollama needs no key. Run a local Ollama server, then select Ollama and a model. For another OpenAI-compatible server, select Custom OpenAI-compatible, enter the base URL (for example `http://localhost:8080/v1/`), and add a key only if the server requires one. The server must expose `/v1/models` and `/v1/chat/completions`; vLLM, LocalAI, LiteLLM, and LM Studio work.

### AI errors

- API key not configured: store the key on the settings page.
- AI is not configured: choose a provider and a model on the settings page.
- Could not load models: check the key, the base URL, and the network. For Ollama, check that the local server is running.
- The request timed out after 60 seconds: try a faster model or check the network.
- The AI could not extract a recipe: use a clearer photo, a more descriptive hint, or a different URL.

## Bring!

YummyBox sends each item to the first shopping list in the Bring! account. You need a Bring! account; the credentials are stored on the settings page or supplied with `BRING_EMAIL` and `BRING_PASSWORD`.

## Appearance and language

The theme button cycles through system, light, and dark. The language button offers System, English, and Deutsch. Both choices are saved in the browser and apply immediately.

## Container permissions

The container runs as UID 1000. When `/data` is a host directory owned by another user, the container cannot create the database and the log shows:

```text
failed to initialize database: database error: error returned from database: (code: 14) unable to open database file
```

Give the directory to the container user:

```bash
sudo chown -R 1000:1000 /path/to/host/data
```

Or run as the directory owner:

```bash
docker run -d -p 11341:11341 -v /path/to/data:/data --user 568:568 ghcr.io/rouhim/yummybox:latest
```

TrueNAS SCALE datasets often default to UID 568. In the TrueNAS Datasets UI, select the dataset, open Edit Permissions, set User and Group to 1000, and apply recursively. In the TrueNAS Apps UI, set Workload Configuration → Run As User to 568 and Run As Group to 568. In `docker-compose.yaml`, set `user: "568:568"`.

Check the owner with `ls -ld /path/to/host/data` and the logs with `docker logs yummybox`. The `using database at /data/yummybox.db` line should no longer be followed by `code: 14`.
