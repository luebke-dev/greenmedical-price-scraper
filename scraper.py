"""
GreenMedical Cannabis Flower Scraper

Scrapes all cannabis flowers from all pharmacies with live stock
on greenmedical.health and writes results to CSV.
"""

import argparse
import csv
import re
import time
from base64 import b64encode
from pathlib import Path
from urllib.parse import urljoin

import requests
from bs4 import BeautifulSoup

BASE_URL = "https://greenmedical.health"
PHARMACY_URL = f"{BASE_URL}/de/cannabis/pharmacy/"
FLOWERS_URL = f"{BASE_URL}/de/cannabis/flowers"

HEADERS = {
    "User-Agent": "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    "Accept-Language": "de,en-US;q=0.7,en;q=0.3",
}
REQUEST_TIMEOUT = 30
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
]


def create_session() -> requests.Session:
    session = requests.Session()
    session.headers.update(HEADERS)
    return session


def get_pharmacies_with_livebestand(session: requests.Session) -> list[dict]:
    """Get all pharmacies that have live stock (first 2 tables on pharmacy page)."""
    resp = session.get(PHARMACY_URL, timeout=REQUEST_TIMEOUT)
    resp.raise_for_status()
    soup = BeautifulSoup(resp.text, "lxml")

    tables = soup.find_all("table")
    pharmacies = []

    # First table only: Partnerapotheken (with live stock)
    for table in tables[:1]:
        for row in table.find_all("tr")[1:]:  # skip header
            cells = row.find_all("td")
            if len(cells) < 4:
                continue
            link = cells[0].find("a")
            if not link:
                continue
            href = link.get("href", "")
            if not href.startswith("http"):
                href = urljoin(BASE_URL, href)
            pharmacies.append({
                "name": link.get_text(strip=True),
                "url": href,
                "plz": cells[1].get_text(strip=True),
                "stadt": cells[2].get_text(strip=True),
                "adresse": cells[3].get_text(strip=True),
            })

    print(f"Found {len(pharmacies)} pharmacies with live stock.")
    return pharmacies


def get_pharmacy_uuid(session: requests.Session, pharmacy_url: str) -> str | None:
    """Extract pharmacy UUID from the pharmacy detail page."""
    resp = session.get(pharmacy_url, timeout=REQUEST_TIMEOUT)
    resp.raise_for_status()
    soup = BeautifulSoup(resp.text, "lxml")

    # Look for "Livebestand Uebersicht" link which contains pharmacyAvailability=<UUID>
    for link in soup.find_all("a", href=True):
        href = link["href"]
        match = re.search(r"pharmacyAvailability=([a-f0-9-]+)", href)
        if match:
            return match.group(1)

    return None


def make_delivery_target(uuid: str) -> str:
    """Encode pharmacy UUID as deliveryTarget parameter."""
    raw = f"pharmacy:|:{uuid}"
    return b64encode(raw.encode()).decode()


def scrape_flowers_for_pharmacy(
    session: requests.Session, pharmacy: dict, delivery_target: str
) -> list[dict]:
    """Scrape all flower pages for a given pharmacy."""
    products = []
    page = 1

    while True:
        params = {
            "deliveryTarget": delivery_target,
            "onlyShowIfAvailable": "1",
            "page": str(page),
        }
        resp = session.get(FLOWERS_URL, params=params, timeout=REQUEST_TIMEOUT)
        resp.raise_for_status()
        soup = BeautifulSoup(resp.text, "lxml")

        tiles = soup.find_all("article", class_="productGridTile")
        if not tiles:
            break

        for tile in tiles:
            product = extract_product(tile)
            product["apotheke"] = pharmacy["name"]
            product["apotheke_plz"] = pharmacy["plz"]
            product["apotheke_stadt"] = pharmacy["stadt"]
            products.append(product)

        # Check pagination
        pag = soup.find("div", class_="paginationContainer")
        if pag:
            pag_text = pag.get_text(strip=True)
            match = re.search(r"(\d+)\s*/\s*(\d+)", pag_text)
            if match:
                current, total = int(match.group(1)), int(match.group(2))
                if current >= total:
                    break
            else:
                break
        else:
            break

        page += 1
        time.sleep(0.5)  # be polite

    return products


def extract_product(tile) -> dict:
    """Extract product data from a single product tile."""
    name = ""
    h2 = tile.find("h2")
    if h2:
        name = h2.get_text(strip=True)

    # THC
    thc_el = tile.find("div", class_=lambda c: c and "flowerTileBadgeThc" in c)
    thc = ""
    if thc_el:
        bold = thc_el.find("span", class_="bold")
        thc = bold.get_text(strip=True) if bold else thc_el.get_text(strip=True)

    # CBD
    cbd_el = tile.find("div", class_=lambda c: c and "flowerTileBadgeCbd" in c)
    cbd = ""
    if cbd_el:
        bold = cbd_el.find("span", class_="bold")
        cbd = bold.get_text(strip=True) if bold else cbd_el.get_text(strip=True)

    # Genetik (Strain)
    strain_el = tile.find("div", class_=lambda c: c and "flowerTileBadgeStrain" in c)
    genetik = strain_el.get_text(strip=True) if strain_el else ""

    # Bezeichnung - find "Bezeichnung" label div then the next bold div
    bezeichnung = ""
    for div in tile.find_all("div", class_="text-uppercase"):
        if "bezeichnung" in div.get_text(strip=True).lower():
            next_bold = div.find_next_sibling("div", class_="bold")
            if next_bold:
                bezeichnung = next_bold.get_text(strip=True)
            break

    # Price
    price_el = tile.find("span", class_=lambda c: c and "productGridTilePriceAmount" in c)
    price = price_el.get_text(strip=True) if price_el else ""

    # Availability
    avail_el = tile.find("div", class_=lambda c: c and "productGridTileStatusAvailability" in c)
    availability = avail_el.get_text(strip=True) if avail_el else ""

    return {
        "name": name,
        "bezeichnung": bezeichnung,
        "genetik": genetik,
        "thc": thc,
        "cbd": cbd,
        "preis_pro_gramm": price,
        "verfuegbarkeit": availability,
    }


def scrape_all_flowers() -> list[dict]:
    """Scrape all available flowers for all pharmacies with live stock."""
    session = create_session()

    print("Fetching pharmacies with live stock...")
    pharmacies = get_pharmacies_with_livebestand(session)

    print("Fetching pharmacy UUIDs...")
    pharmacy_targets = []
    for i, pharmacy in enumerate(pharmacies):
        uuid = get_pharmacy_uuid(session, pharmacy["url"])
        if uuid:
            delivery_target = make_delivery_target(uuid)
            pharmacy_targets.append((pharmacy, delivery_target))
            print(f"  [{i+1}/{len(pharmacies)}] {pharmacy['name']}: UUID found")
        else:
            print(f"  [{i+1}/{len(pharmacies)}] {pharmacy['name']}: no UUID, skipping")
        time.sleep(0.3)

    print(f"\n{len(pharmacy_targets)} pharmacies with valid UUIDs. Starting scrape...")

    all_products = []
    for i, (pharmacy, delivery_target) in enumerate(pharmacy_targets):
        print(f"  [{i+1}/{len(pharmacy_targets)}] Scraping {pharmacy['name']}...")
        products = scrape_flowers_for_pharmacy(session, pharmacy, delivery_target)
        all_products.extend(products)
        print(f"    -> {len(products)} flowers found")
        time.sleep(0.5)

    return all_products


def write_products_csv(products: list[dict], output_file: Path) -> None:
    output_file.parent.mkdir(parents=True, exist_ok=True)
    with open(output_file, "w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=FIELDNAMES)
        writer.writeheader()
        writer.writerows(products)


def parse_args(argv: list[str] | None = None) -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Scrape GreenMedical flower prices.")
    parser.add_argument(
        "--output",
        default="greenmedical_flowers.csv",
        help="CSV output path. Defaults to greenmedical_flowers.csv.",
    )
    return parser.parse_args(argv)


def main(argv: list[str] | None = None):
    args = parse_args(argv)
    output_file = Path(args.output)
    all_products = scrape_all_flowers()
    write_products_csv(all_products, output_file)
    print(f"\nDone! {len(all_products)} products written to {output_file}")


if __name__ == "__main__":
    main()
