"""Shared CSV column contract between scraper.py and build_site.py.

The scraper writes these columns and build_site reads them; this list is the
single source of truth for name and order. Nothing else lives here.
"""

FIELDNAMES = [
    "apotheke",
    "apotheke_plz",
    "apotheke_stadt",
    "name",
    "bezeichnung",
    "genetik",
    "thc",
    "cbd",
    "preis_pro_gramm",
    "verfuegbarkeit",
    "produkt_url",
]
