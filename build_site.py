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

DEFAULT_INPUT = "greenmedical_flowers.csv"
DEFAULT_OUTPUT_DIR = "dist"
DATA_DIR_NAME = "data"
CSV_OUTPUT_NAME = "greenmedical_flowers.csv"
JSON_OUTPUT_NAME = "flowers.json"
METADATA_OUTPUT_NAME = "metadata.json"


TEMPLATE_PATH = Path(__file__).resolve().parent / "templates" / "index.html"


def load_template() -> str:
    return TEMPLATE_PATH.read_text(encoding="utf-8")


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


def read_flowers(input_file: Path) -> list[dict]:
    flowers = []

    with input_file.open(newline="", encoding="utf-8") as csv_file:
        for index, row in enumerate(csv.DictReader(csv_file), start=1):
            item = {
                "id": index,
                "apotheke": clean_text(row.get("apotheke")),
                "apotheke_plz": clean_text(row.get("apotheke_plz")),
                "apotheke_stadt": clean_text(row.get("apotheke_stadt")),
                "name": clean_text(row.get("name")),
                "bezeichnung": clean_text(row.get("bezeichnung")),
                "genetik": clean_text(row.get("genetik")),
                "thc": clean_text(row.get("thc")),
                "cbd": clean_text(row.get("cbd")),
                "preis_pro_gramm": clean_text(row.get("preis_pro_gramm")),
                "verfuegbarkeit": clean_text(row.get("verfuegbarkeit")),
            }
            item["preis_eur_pro_gramm"] = parse_decimal(item["preis_pro_gramm"])
            thc_percent = parse_percent(item["thc"])
            thc_price = calculate_thc_price(item["preis_eur_pro_gramm"], thc_percent)
            item["preis_eur_pro_gramm_thc"] = thc_price
            item["sort"] = {
                "price": item["preis_eur_pro_gramm"],
                "price_per_thc_gram": thc_price,
                "thc": thc_percent,
                "cbd": parse_percent(item["cbd"]),
            }
            thc_price_search = "" if thc_price is None else f"{thc_price:.2f} €/g thc"
            item["search"] = " ".join(
                [
                    item["apotheke"],
                    item["apotheke_plz"],
                    item["apotheke_stadt"],
                    item["name"],
                    item["bezeichnung"],
                    item["genetik"],
                    item["thc"],
                    item["cbd"],
                    item["preis_pro_gramm"],
                    thc_price_search,
                    item["verfuegbarkeit"],
                ]
            ).lower()
            flowers.append(item)

    return flowers


def build_metadata(flowers: list[dict]) -> dict:
    prices = [
        item["preis_eur_pro_gramm"]
        for item in flowers
        if item["preis_eur_pro_gramm"] is not None
    ]
    pharmacies = {item["apotheke"] for item in flowers if item["apotheke"]}
    strains = {
        (item["name"], item["bezeichnung"])
        for item in flowers
        if item["name"] or item["bezeichnung"]
    }

    return {
        "generated_at": datetime.now(UTC).isoformat(),
        "source": "https://greenmedical.health/de/cannabis/flowers",
        "total": len(flowers),
        "pharmacy_count": len(pharmacies),
        "strain_count": len(strains),
        "lowest_price": min(prices) if prices else None,
    }


def write_json(path: Path, payload) -> None:
    path.write_text(
        json.dumps(payload, ensure_ascii=False, indent=2, sort_keys=True),
        encoding="utf-8",
    )


def build_site(input_file: Path, output_dir: Path) -> None:
    if not input_file.exists():
        raise FileNotFoundError(f"Input CSV not found: {input_file}")

    output_dir.mkdir(parents=True, exist_ok=True)
    data_dir = output_dir / DATA_DIR_NAME
    data_dir.mkdir(parents=True, exist_ok=True)

    flowers = read_flowers(input_file)
    metadata = build_metadata(flowers)

    shutil.copyfile(input_file, data_dir / CSV_OUTPUT_NAME)
    write_json(data_dir / JSON_OUTPUT_NAME, flowers)
    write_json(data_dir / METADATA_OUTPUT_NAME, metadata)

    generated_at = datetime.fromisoformat(metadata["generated_at"])
    generated_label = generated_at.strftime("%Y-%m-%d %H:%M UTC")
    lowest_price = metadata["lowest_price"]
    lowest_price_label = "" if lowest_price is None else f"{lowest_price:.2f} €/g"

    html = (
        load_template().replace("__GENERATED_LABEL__", generated_label)
        .replace("__TOTAL__", str(metadata["total"]))
        .replace("__PHARMACIES__", str(metadata["pharmacy_count"]))
        .replace("__STRAINS__", str(metadata["strain_count"]))
        .replace("__LOWEST_PRICE__", lowest_price_label)
    )
    (output_dir / "index.html").write_text(html, encoding="utf-8")


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
