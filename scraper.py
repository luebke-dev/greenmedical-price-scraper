"""
GreenMedical Cannabis Flower Scraper

Scrapes all cannabis flowers from all pharmacies with live stock
on greenmedical.health and writes results to CSV.
"""

import argparse
import csv
import logging
import re
import time
from base64 import b64encode
from pathlib import Path
from urllib.parse import parse_qsl, urlencode, urljoin, urlparse, urlunparse

import requests
from bs4 import BeautifulSoup
from requests.adapters import HTTPAdapter
from urllib3.util.retry import Retry

from csv_fields import FIELDNAMES

logger = logging.getLogger("greenmedical.scraper")

BASE_URL = "https://greenmedical.health"
PHARMACY_URL = f"{BASE_URL}/de/cannabis/pharmacy/"
FLOWERS_URL = f"{BASE_URL}/de/cannabis/flowers"

HEADERS = {
    "User-Agent": "Mozilla/5.0 (X11; Linux x86_64; rv:128.0) Gecko/20100101 Firefox/128.0",
    "Accept": "text/html,application/xhtml+xml,application/xml;q=0.9,*/*;q=0.8",
    "Accept-Language": "de,en-US;q=0.7,en;q=0.3",
}
REQUEST_TIMEOUT = 30

# Retry/backoff for transient errors and rate limiting.
RETRY_TOTAL = 4
RETRY_BACKOFF_FACTOR = 1.0  # sleeps: 0s, 2s, 4s, 8s ...
RETRY_STATUS_FORCELIST = (429, 500, 502, 503, 504)

# Politeness delays between requests (seconds).
PHARMACY_DELAY = 0.3
PAGE_DELAY = 0.5


def create_session() -> requests.Session:
    session = requests.Session()
    session.headers.update(HEADERS)

    retry = Retry(
        total=RETRY_TOTAL,
        backoff_factor=RETRY_BACKOFF_FACTOR,
        status_forcelist=RETRY_STATUS_FORCELIST,
        allowed_methods=frozenset(["GET"]),
        respect_retry_after_header=True,
        raise_on_status=False,
    )
    adapter = HTTPAdapter(max_retries=retry)
    session.mount("https://", adapter)
    session.mount("http://", adapter)
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

    logger.info("Found %d pharmacies with live stock.", len(pharmacies))
    return pharmacies


def get_pharmacy_uuid(session: requests.Session, pharmacy_url: str) -> str | None:
    """Extract pharmacy UUID from the pharmacy detail page."""
    resp = session.get(pharmacy_url, timeout=REQUEST_TIMEOUT)
    resp.raise_for_status()
    soup = BeautifulSoup(resp.text, "lxml")

    # Look for "Livebestand Übersicht" link which contains pharmacyAvailability=<UUID>
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


def with_delivery_target(url: str, delivery_target: str) -> str:
    """Add/replace the deliveryTarget query param so the link opens at this pharmacy."""
    parts = urlparse(url)
    query = dict(parse_qsl(parts.query))
    query["deliveryTarget"] = delivery_target
    return urlunparse(parts._replace(query=urlencode(query)))


def parse_pagination(soup) -> tuple[int, int] | None:
    """Read (current, total) from the "n / m" pagination container, if present."""
    pag = soup.find("div", class_="paginationContainer")
    if not pag:
        return None
    match = re.search(r"(\d+)\s*/\s*(\d+)", pag.get_text(strip=True))
    if not match:
        return None
    return int(match.group(1)), int(match.group(2))


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
            if product["produkt_url"]:
                product["produkt_url"] = with_delivery_target(
                    product["produkt_url"], delivery_target
                )
            products.append(product)

        pagination = parse_pagination(soup)
        if pagination is None:
            break
        current, total = pagination
        if current >= total:
            break

        page += 1
        time.sleep(PAGE_DELAY)  # be polite

    return products


def extract_badge_value(tile, badge_class: str) -> str:
    """Extract the value from a flower tile badge (THC/CBD), preferring the bold span."""
    badge = tile.find("div", class_=lambda c: c and badge_class in c)
    if not badge:
        return ""
    bold = badge.find("span", class_="bold")
    return bold.get_text(strip=True) if bold else badge.get_text(strip=True)


def extract_product_url(tile, h2) -> str:
    """Find the product detail link in a tile (prefers the anchor around the title)."""
    href = ""
    if h2:
        anchor = h2.find_parent("a") or h2.find("a", href=True)
        if anchor and anchor.get("href"):
            href = anchor["href"]
    if not href:
        anchor = tile.find("a", href=True)
        if anchor:
            href = anchor["href"]
    return urljoin(BASE_URL, href) if href else ""


def extract_product(tile) -> dict:
    """Extract product data from a single product tile."""
    name = ""
    h2 = tile.find("h2")
    if h2:
        name = h2.get_text(strip=True)

    thc = extract_badge_value(tile, "flowerTileBadgeThc")
    cbd = extract_badge_value(tile, "flowerTileBadgeCbd")

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
        "produkt_url": extract_product_url(tile, h2),
    }


def resolve_pharmacy_targets(
    session: requests.Session, pharmacies: list[dict]
) -> list[tuple[dict, str]]:
    """Look up each pharmacy's UUID and encode it as a deliveryTarget.

    Pharmacies whose UUID cannot be fetched or found are logged and skipped.
    """
    targets = []
    for i, pharmacy in enumerate(pharmacies):
        try:
            uuid = get_pharmacy_uuid(session, pharmacy["url"])
        except requests.RequestException as exc:
            logger.warning(
                "  [%d/%d] %s: failed to fetch UUID (%s), skipping",
                i + 1, len(pharmacies), pharmacy["name"], exc,
            )
            continue

        if uuid:
            targets.append((pharmacy, make_delivery_target(uuid)))
            logger.info(
                "  [%d/%d] %s: UUID found", i + 1, len(pharmacies), pharmacy["name"]
            )
        else:
            logger.info(
                "  [%d/%d] %s: no UUID, skipping",
                i + 1, len(pharmacies), pharmacy["name"],
            )
        time.sleep(PHARMACY_DELAY)

    return targets


def scrape_pharmacy_targets(
    session: requests.Session, targets: list[tuple[dict, str]]
) -> list[dict]:
    """Scrape flowers for each (pharmacy, deliveryTarget) pair, skipping failures."""
    all_products = []
    failed = 0
    for i, (pharmacy, delivery_target) in enumerate(targets):
        logger.info(
            "  [%d/%d] Scraping %s...", i + 1, len(targets), pharmacy["name"]
        )
        try:
            products = scrape_flowers_for_pharmacy(session, pharmacy, delivery_target)
        except requests.RequestException as exc:
            failed += 1
            logger.warning(
                "    -> failed to scrape %s (%s), skipping",
                pharmacy["name"], exc,
            )
            continue
        all_products.extend(products)
        logger.info("    -> %d flowers found", len(products))
        time.sleep(PAGE_DELAY)

    if failed:
        logger.warning("%d pharmacies could not be scraped and were skipped.", failed)

    return all_products


def scrape_all_flowers() -> list[dict]:
    """Scrape all available flowers for all pharmacies with live stock.

    Failures for an individual pharmacy are logged and skipped so that a single
    network error or layout change does not abort the entire run.
    """
    with create_session() as session:
        logger.info("Fetching pharmacies with live stock...")
        pharmacies = get_pharmacies_with_livebestand(session)

        logger.info("Fetching pharmacy UUIDs...")
        targets = resolve_pharmacy_targets(session, pharmacies)
        logger.info("%d pharmacies with valid UUIDs. Starting scrape...", len(targets))

        return scrape_pharmacy_targets(session, targets)


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
    logging.basicConfig(
        level=logging.INFO,
        format="%(asctime)s %(levelname)s %(message)s",
        datefmt="%H:%M:%S",
    )
    output_file = Path(args.output)
    all_products = scrape_all_flowers()
    write_products_csv(all_products, output_file)
    logger.info("Done! %d products written to %s", len(all_products), output_file)


if __name__ == "__main__":
    main()
