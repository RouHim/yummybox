<p align="center">
  <img src=".github/readme/banner.svg" width="600" alt="MealMe — local-first meal manager">
</p>

<p align="center">
  <a href="https://github.com/RouHim/mealme/releases/latest"><img src="https://img.shields.io/github/v/release/RouHim/mealme?label=latest" alt="Latest release"></a>
  <a href="https://github.com/RouHim/mealme/releases/latest"><img src="https://img.shields.io/badge/platform-linux%20%7C%20macOS%20%7C%20Windows-blue" alt="Platform: linux | macOS | Windows"></a>
  <a href="https://opensource.org/licenses/MIT"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="License: MIT"></a>
  <a href="https://github.com/RouHim/mealme/actions/workflows/ci.yml"><img src="https://github.com/RouHim/mealme/actions/workflows/ci.yml/badge.svg" alt="CI/CD"></a>
</p>

<p align="center">
  <img src=".github/readme/screenshot.png" width="720" alt="MealMe meals screen showing 10 meals with images">
</p>

**MealMe** is a personal meal manager that runs entirely on your computer — no cloud, no accounts, no subscriptions. Add meals, search your collection, plan your week, and import recipes from the web or from photos using AI.

## What you can do

- **Manage your meals** — add, edit, search, and delete meals. Each meal has a name, ingredient list with quantities, plus auto-tracked creation and update times.
- **Plan your week** — generate a weekly meal plan from your collection. Plans automatically aggregate ingredients so you know exactly what to shop for.
- **Import recipes** — paste a URL, drop in raw HTML, or let AI parse a recipe from a photo or text description. Imported recipes land in a review screen before saving.
- **Run anywhere** — single binary, no dependencies. Data lives in a SQLite file on your disk; you control it.
- **Light and dark themes** — follows your system preference, with a manual toggle that remembers your choice.

## Getting started

### Download a pre-built binary

Grab the latest release for your platform from the [Releases page](https://github.com/RouHim/mealme/releases/latest):

```bash
# Linux / macOS (x86_64)
curl -L -o mealme https://github.com/RouHim/mealme/releases/latest/download/mealme-x86_64-unknown-linux-musl
chmod +x mealme
./mealme

# Linux / macOS (arm64 / Apple Silicon)
curl -L -o mealme https://github.com/RouHim/mealme/releases/latest/download/mealme-aarch64-unknown-linux-musl
chmod +x mealme
./mealme
```

Then open **http://127.0.0.1:11341** in your browser.

### Build from source

Requires [Rust](https://rustup.rs) 1.85+ and [Node.js](https://nodejs.org) 26+ (build-time only — not needed to run).

```bash
git clone https://github.com/RouHim/mealme.git
cd mealme
cargo run --release
```

## Using MealMe

### Meals

The home screen shows your meal collection, newest first. Use the search bar to filter by name or ingredient. Click a meal to edit it, or use the delete button to remove it.

When adding or editing a meal, each ingredient goes on its own line with an optional quantity:

```
200g pasta
2 eggs
salt to taste
```

Quantity text (e.g. `200g`, `2 cups`, `a pinch`) is preserved and used by the planner to sum up your shopping list.

### Weekly planner

Switch to the **Planner** tab to generate a weekly meal plan. Pick a week, choose how many meals you want, and the planner randomly selects them from your collection. You can swap individual meals or regenerate the whole plan.

The ingredient summary merges identical ingredients across all planned meals and sums numeric quantities — ready for your shopping list.

### Recipe import

**From URL** — paste a link to any recipe website. MealMe fetches the page and extracts the recipe automatically.

**From paste** — drop in raw HTML or JSON-LD markup if you already have the source.

**From photo or text (AI)** — attach a photo of a dish or recipe card, optionally add a text hint, and a vision-capable LLM parses it into a structured recipe. This requires an API key from a supported provider (see [Configuration](#configuration)).

All import methods use the same review screen — you can edit the extracted recipe before saving.

## Configuration

| Variable | What it does | Default |
|----------|--------------|---------|
| `MEALME_DATA_DIR` | Where the database file lives | `./data` (next to the binary) |

### LLM providers

To use the AI-powered recipe import, set an API key for your provider:

| Provider | Environment variable | Example model |
|----------|---------------------|---------------|
| OpenAI | `OPENAI_API_KEY` | `gpt-4o-mini` |
| Anthropic | `ANTHROPIC_API_KEY` | `claude-sonnet-4-20250514` |
| Google | `GOOGLE_API_KEY` | `gemini-2.5-flash` |
| Groq | `GROQ_API_KEY` | `llama-4-maverick-17b-128e-instruct` |
| Ollama (local) | _(none — uses local Ollama server)_ | `llama3.2-vision` |

Example:

```bash
OPENAI_API_KEY=sk-... ./mealme
```

### Bring! shopping list

Send ingredients from your weekly plan directly to your [Bring!](https://getbring.com) shopping list. Each ingredient gets a one-click "Send to Bring!" button next to it in the planner's ingredient summary.

If you signed up with Google, Apple, or Facebook, you need to set a password first — the Bring! API doesn't support social login tokens:

- **Mobile**: open the Bring! app → Profile → More settings → Change password
- **Web**: visit [getbring.com](https://web.getbring.com) → Settings → Reset password

You can still log in with Google/Apple/Facebook afterward — the password is an additional credential.

| Variable | What it does |
|----------|--------------|
| `BRING_EMAIL` | Your Bring! account email address |
| `BRING_PASSWORD` | Your Bring! account password |

Example:

```bash
BRING_EMAIL=you@example.com BRING_PASSWORD=your-password ./mealme
```

## Contributing

Bug reports and pull requests are welcome. See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

MIT — see [LICENSE](LICENSE).
