# GreenMedical Price Scraper

Scrapes live cannabis flower availability from GreenMedical partner pharmacies and
publishes the results as a static GitHub Pages table.

The generated page includes:

- full-text search across strain, product name, pharmacy, city, THC, CBD, price, and status
- sortable columns
- CSV and JSON downloads
- automatic GitHub Pages publishing from GitHub Actions

## Local Usage

Install dependencies:

```bash
python -m pip install .
```

Scrape current data:

```bash
python -u scraper.py --output greenmedical_flowers.csv
```

Build the static site:

```bash
python build_site.py --input greenmedical_flowers.csv --output dist
```

Preview locally:

```bash
python -m http.server 8000 --directory dist
```

Then open <http://localhost:8000>.

## GitHub Pages

The workflow in `.github/workflows/pages.yml` runs on pushes to `main`, once per
day, and manually via `workflow_dispatch`. It scrapes fresh data, builds `dist/`,
and deploys the folder to GitHub Pages.

In the GitHub repository settings, set Pages to use GitHub Actions as the source
if it is not selected automatically.
