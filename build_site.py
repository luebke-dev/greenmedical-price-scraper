"""
Build a static, searchable GreenMedical price table for GitHub Pages.
"""

from __future__ import annotations

import argparse
import csv
import json
import re
import shutil
from datetime import UTC, datetime
from decimal import Decimal
from pathlib import Path

from csv_fields import FIELDNAMES

DEFAULT_INPUT = "greenmedical_flowers.csv"
DEFAULT_OUTPUT_DIR = "dist"
DATA_DIR_NAME = "data"
CSV_OUTPUT_NAME = "greenmedical_flowers.csv"
JSON_OUTPUT_NAME = "flowers.json"
METADATA_OUTPUT_NAME = "metadata.json"


# Static site assets, copied verbatim into the output directory. The page is
# fully data-driven: all dynamic values flow through data/*.json and are
# rendered client-side. If a server-rendered value is ever needed again
# (e.g. OpenGraph tags), use stdlib string.Template.substitute — it raises on
# missing keys instead of silently shipping literal placeholder text.
SITE_DIR = Path(__file__).resolve().parent / "site"


def parse_decimal(value: str) -> float | None:
    match = re.search(r"(\d+(?:[.,]\d+)?)", value.replace("\xa0", " "))
    if not match:
        return None
    return float(Decimal(match.group(1).replace(",", ".")))


def parse_percent(value: str) -> float | None:
    stripped = value.strip()
    if not stripped:
        return None

    match = re.search(r"(\d+(?:[.,]\d+)?)", stripped)
    if not match:
        return None

    parsed = float(Decimal(match.group(1).replace(",", ".")))
    if stripped.startswith("<"):
        return max(0, parsed - 0.01)
    return parsed


def calculate_thc_price(price: float | None, thc_percent: float | None) -> float | None:
    if price is None or thc_percent is None or thc_percent <= 0:
        return None
    return round(price / (thc_percent / 100), 2)


def clean_text(value: str | None) -> str:
    return " ".join((value or "").replace("\xa0", " ").split())


def read_offers(input_file: Path) -> list[dict]:
    """Read the raw CSV into one record per pharmacy offer, with parsed values."""
    offers = []

    with input_file.open(newline="", encoding="utf-8") as csv_file:
        for row in csv.DictReader(csv_file):
            offer = {field: clean_text(row.get(field)) for field in FIELDNAMES}
            # URLs must not have inner whitespace collapsed.
            offer["produkt_url"] = (row.get("produkt_url") or "").strip()
            price = parse_decimal(offer["preis_pro_gramm"])
            thc_percent = parse_percent(offer["thc"])
            cbd_percent = parse_percent(offer["cbd"])
            offer["preis_eur_pro_gramm"] = price
            offer["preis_eur_pro_gramm_thc"] = calculate_thc_price(price, thc_percent)
            offer["preis_eur_pro_gramm_cbd"] = calculate_thc_price(price, cbd_percent)
            offer["thc_value"] = thc_percent
            offer["cbd_value"] = cbd_percent
            offers.append(offer)

    return offers


def _first_nonempty(values) -> str:
    for value in values:
        if value:
            return value
    return ""


def _first_not_none(values):
    for value in values:
        if value is not None:
            return value
    return None


def group_by_strain(offers: list[dict]) -> list[dict]:
    """Deduplicate offers into one record per strain (name + Bezeichnung).

    Each strain lists the pharmacies offering it (sorted cheapest first), so the
    same strain sold by several pharmacies collapses into a single grouped entry.
    """
    groups: dict[tuple[str, str], list[dict]] = {}
    for offer in offers:
        key = (offer["name"].casefold(), offer["bezeichnung"].casefold())
        groups.setdefault(key, []).append(offer)

    strains = []
    for index, key in enumerate(sorted(groups), start=1):
        members = groups[key]
        members_sorted = sorted(
            members,
            key=lambda o: (
                o["preis_eur_pro_gramm"] is None,
                o["preis_eur_pro_gramm"] or 0.0,
            ),
        )

        prices = [o["preis_eur_pro_gramm"] for o in members if o["preis_eur_pro_gramm"] is not None]
        thc_prices = [
            o["preis_eur_pro_gramm_thc"]
            for o in members
            if o["preis_eur_pro_gramm_thc"] is not None
        ]
        min_price = min(prices) if prices else None
        min_thc_price = min(thc_prices) if thc_prices else None

        name = _first_nonempty(o["name"] for o in members)
        bezeichnung = _first_nonempty(o["bezeichnung"] for o in members)
        genetik = _first_nonempty(o["genetik"] for o in members)
        thc = _first_nonempty(o["thc"] for o in members)
        cbd = _first_nonempty(o["cbd"] for o in members)

        offer_records = [
            {
                "apotheke": o["apotheke"],
                "apotheke_plz": o["apotheke_plz"],
                "apotheke_stadt": o["apotheke_stadt"],
                "preis_pro_gramm": o["preis_pro_gramm"],
                "preis_eur_pro_gramm": o["preis_eur_pro_gramm"],
                "preis_eur_pro_gramm_thc": o["preis_eur_pro_gramm_thc"],
                "verfuegbarkeit": o["verfuegbarkeit"],
                "produkt_url": o["produkt_url"],
            }
            for o in members_sorted
        ]

        offer_search = " ".join(
            part
            for o in members
            for part in (
                o["apotheke"],
                o["apotheke_plz"],
                o["apotheke_stadt"],
                o["preis_pro_gramm"],
                o["verfuegbarkeit"],
            )
        )
        thc_price_search = "" if min_thc_price is None else f"{min_thc_price:.2f} €/g thc"

        strains.append(
            {
                "id": index,
                "name": name,
                "bezeichnung": bezeichnung,
                "genetik": genetik,
                "thc": thc,
                "cbd": cbd,
                "min_price": min_price,
                "min_price_per_thc_gram": min_thc_price,
                "pharmacy_count": len({o["apotheke"] for o in members if o["apotheke"]}),
                "offers": offer_records,
                "sort": {
                    "price": min_price,
                    "price_per_thc_gram": min_thc_price,
                    "thc": _first_not_none(o["thc_value"] for o in members),
                    "cbd": _first_not_none(o["cbd_value"] for o in members),
                },
                "search": " ".join(
                    [name, bezeichnung, genetik, thc, cbd, offer_search, thc_price_search]
                ).lower(),
            }
        )

    return strains


def _highlight(offer: dict, price: float | None) -> dict:
    return {
        "price": price,
        "name": offer["name"],
        "apotheke": offer["apotheke"],
        "genetik": offer["genetik"],
        "thc": offer["thc"],
        "cbd": offer["cbd"],
        "produkt_url": offer.get("produkt_url", ""),
    }


def _cheapest(offers: list[dict], key: str) -> dict | None:
    """Pick the offer with the lowest value for `key`, with its strain and pharmacy."""
    candidates = [offer for offer in offers if offer.get(key) is not None]
    if not candidates:
        return None
    best = min(candidates, key=lambda offer: offer[key])
    return _highlight(best, best[key])


def _highest(offers: list[dict], value_fn) -> dict | None:
    """Pick the offer with the highest value_fn, breaking ties by cheapest price."""
    candidates = [(value_fn(offer), offer) for offer in offers]
    candidates = [(value, offer) for value, offer in candidates if value is not None]
    if not candidates:
        return None
    value, best = max(
        candidates,
        key=lambda pair: (pair[0], -(pair[1]["preis_eur_pro_gramm"] or float("inf"))),
    )
    return _highlight(best, best["preis_eur_pro_gramm"])


def _combined_cannabinoids(offer: dict) -> float | None:
    """Reward strains high in THC *and* CBD at once via the product of the two.

    A sum would be dominated by THC, so a high-THC/near-zero-CBD strain would
    win; the product collapses toward zero unless both values are substantial.
    """
    thc, cbd = offer["thc_value"], offer["cbd_value"]
    if thc is None or cbd is None:
        return None
    return thc * cbd


def build_metadata(offers: list[dict], strains: list[dict]) -> dict:
    pharmacies = {offer["apotheke"] for offer in offers if offer["apotheke"]}
    cheapest_gram = _cheapest(offers, "preis_eur_pro_gramm")

    return {
        "generated_at": datetime.now(UTC).isoformat(),
        "source": "https://greenmedical.health/de/cannabis/flowers",
        "total": len(offers),
        "pharmacy_count": len(pharmacies),
        "strain_count": len(strains),
        "lowest_price": cheapest_gram["price"] if cheapest_gram else None,
        "cheapest_gram": cheapest_gram,
        "cheapest_thc_gram": _cheapest(offers, "preis_eur_pro_gramm_thc"),
        "cheapest_cbd_gram": _cheapest(offers, "preis_eur_pro_gramm_cbd"),
        "highest_thc": _highest(offers, lambda offer: offer["thc_value"]),
        "highest_cbd": _highest(offers, lambda offer: offer["cbd_value"]),
        "highest_thc_cbd": _highest(offers, _combined_cannabinoids),
    }


def write_json(path: Path, payload) -> None:
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True),
        encoding="utf-8",
    )


def build_site(input_file: Path, output_dir: Path) -> None:
    if not input_file.exists():
        raise FileNotFoundError(f"Input CSV not found: {input_file}")

    offers = read_offers(input_file)
    strains = group_by_strain(offers)
    metadata = build_metadata(offers, strains)

    shutil.copytree(SITE_DIR, output_dir, dirs_exist_ok=True)

    data_dir = output_dir / DATA_DIR_NAME
    data_dir.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(input_file, data_dir / CSV_OUTPUT_NAME)
    write_json(data_dir / JSON_OUTPUT_NAME, strains)
    write_json(data_dir / METADATA_OUTPUT_NAME, metadata)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Build the static GreenMedical table.")
    parser.add_argument(
        "--input",
        default=DEFAULT_INPUT,
        help=f"Input CSV path. Defaults to {DEFAULT_INPUT}.",
    )
    parser.add_argument(
        "--output",
        default=DEFAULT_OUTPUT_DIR,
        help=f"Output directory. Defaults to {DEFAULT_OUTPUT_DIR}.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None) -> None:
    args = parse_args(argv)
    build_site(Path(args.input), Path(args.output))
    print(f"Static site written to {args.output}")


if __name__ == "__main__":
    main()
