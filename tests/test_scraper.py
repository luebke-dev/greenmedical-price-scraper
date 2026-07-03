"""Unit tests for scraper parsing helpers (no network access)."""

from base64 import b64decode

from bs4 import BeautifulSoup

import csv_fields
import scraper


def _tile(html: str):
    return BeautifulSoup(html, "lxml").find("article")


class TestFieldnamesContract:
    def test_scraper_shares_the_csv_fields_list(self):
        assert scraper.FIELDNAMES is csv_fields.FIELDNAMES

    def test_extract_product_keys_cover_fieldnames(self):
        product = scraper.extract_product(_tile("<article></article>"))
        pharmacy_fields = {"apotheke", "apotheke_plz", "apotheke_stadt"}
        assert set(product) | pharmacy_fields == set(csv_fields.FIELDNAMES)


class TestExtractBadgeValue:
    def test_prefers_bold_span(self):
        tile = _tile(
            '<article><div class="flowerTileBadgeThc">THC '
            '<span class="bold">20%</span></div></article>'
        )
        assert scraper.extract_badge_value(tile, "flowerTileBadgeThc") == "20%"

    def test_falls_back_to_full_text(self):
        tile = _tile('<article><div class="flowerTileBadgeCbd">CBD 1%</div></article>')
        assert scraper.extract_badge_value(tile, "flowerTileBadgeCbd") == "CBD 1%"

    def test_missing_badge_returns_empty_string(self):
        tile = _tile("<article></article>")
        assert scraper.extract_badge_value(tile, "flowerTileBadgeThc") == ""


class TestExtractProduct:
    def test_extracts_all_fields(self):
        tile = _tile(
            """
            <article class="productGridTile">
              <a href="/de/cannabis/flowers/test-bluete"><h2>Test Blüte 20/1</h2></a>
              <div class="flowerTileBadgeThc">THC <span class="bold">20%</span></div>
              <div class="flowerTileBadgeCbd">CBD <span class="bold">1%</span></div>
              <div class="flowerTileBadgeStrain">Indica</div>
              <div class="text-uppercase">Bezeichnung</div>
              <div class="bold">EMK</div>
              <span class="productGridTilePriceAmount">9,50 €</span>
              <div class="productGridTileStatusAvailability">verfügbar</div>
            </article>
            """
        )
        product = scraper.extract_product(tile)
        assert product == {
            "name": "Test Blüte 20/1",
            "bezeichnung": "EMK",
            "genetik": "Indica",
            "thc": "20%",
            "cbd": "1%",
            "preis_pro_gramm": "9,50 €",
            "verfuegbarkeit": "verfügbar",
            "produkt_url": "https://greenmedical.health/de/cannabis/flowers/test-bluete",
        }

    def test_missing_fields_default_to_empty(self):
        product = scraper.extract_product(_tile('<article class="productGridTile"></article>'))
        assert product == {
            "name": "",
            "bezeichnung": "",
            "genetik": "",
            "thc": "",
            "cbd": "",
            "preis_pro_gramm": "",
            "verfuegbarkeit": "",
            "produkt_url": "",
        }


class TestExtractProductUrl:
    def test_prefers_anchor_around_title(self):
        tile = _tile(
            '<article><a href="/de/cannabis/flowers/x"><h2>Name</h2></a>'
            '<a href="/other">more</a></article>'
        )
        h2 = tile.find("h2")
        assert scraper.extract_product_url(tile, h2) == "https://greenmedical.health/de/cannabis/flowers/x"

    def test_falls_back_to_first_anchor(self):
        tile = _tile('<article><h2>Name</h2><a href="/de/cannabis/flowers/y">link</a></article>')
        h2 = tile.find("h2")
        assert scraper.extract_product_url(tile, h2) == "https://greenmedical.health/de/cannabis/flowers/y"

    def test_no_anchor_returns_empty(self):
        tile = _tile("<article><h2>Name</h2></article>")
        assert scraper.extract_product_url(tile, tile.find("h2")) == ""


class TestWithDeliveryTarget:
    def test_appends_delivery_target(self):
        base = "https://greenmedical.health/de/cannabis/flowers/x"
        url = scraper.with_delivery_target(base, "TOKEN")
        assert url == f"{base}?deliveryTarget=TOKEN"

    def test_replaces_existing_delivery_target(self):
        url = scraper.with_delivery_target(
            "https://greenmedical.health/p?deliveryTarget=old&foo=bar", "NEW"
        )
        assert "deliveryTarget=NEW" in url
        assert "deliveryTarget=old" not in url
        assert "foo=bar" in url


class TestMakeDeliveryTarget:
    def test_round_trips_uuid(self):
        uuid = "abc123-def456"
        encoded = scraper.make_delivery_target(uuid)
        assert b64decode(encoded).decode() == f"pharmacy:|:{uuid}"


class TestCreateSession:
    def test_mounts_retry_adapter(self):
        session = scraper.create_session()
        try:
            adapter = session.get_adapter("https://greenmedical.health")
            retry = adapter.max_retries
            assert retry.total == scraper.RETRY_TOTAL
            assert 429 in retry.status_forcelist
        finally:
            session.close()


def _soup(html: str):
    return BeautifulSoup(html, "lxml")


class TestParsePagination:
    def test_reads_current_and_total(self):
        soup = _soup('<div class="paginationContainer">Seite 2 / 7</div>')
        assert scraper.parse_pagination(soup) == (2, 7)

    def test_missing_container_returns_none(self):
        assert scraper.parse_pagination(_soup("<div>kein Pager</div>")) is None

    def test_container_without_ratio_returns_none(self):
        soup = _soup('<div class="paginationContainer">weiter</div>')
        assert scraper.parse_pagination(soup) is None


def _flower_page(name: str, href: str, pagination: str) -> str:
    return f"""
    <html><body>
      <article class="productGridTile">
        <a href="{href}"><h2>{name}</h2></a>
        <span class="productGridTilePriceAmount">9,50 €</span>
      </article>
      <div class="paginationContainer">{pagination}</div>
    </body></html>
    """


class _StubResponse:
    def __init__(self, text: str):
        self.text = text

    def raise_for_status(self):
        pass


class _StubSession:
    """Stands in for requests.Session; serves canned pages keyed by page param."""

    def __init__(self, pages: list[str]):
        self.pages = pages
        self.requested_pages = []

    def get(self, url, params=None, timeout=None):
        page = int(params["page"])
        self.requested_pages.append(page)
        return _StubResponse(self.pages[page - 1])


class TestScrapeFlowersForPharmacy:
    def test_walks_all_pages_and_injects_pharmacy_fields(self, monkeypatch):
        monkeypatch.setattr(scraper, "PAGE_DELAY", 0)
        session = _StubSession([
            _flower_page("Sorte A", "/de/cannabis/flowers/a", "1 / 2"),
            _flower_page("Sorte B", "/de/cannabis/flowers/b", "2 / 2"),
        ])
        pharmacy = {"name": "Adler Apotheke", "plz": "10115", "stadt": "Berlin"}

        products = scraper.scrape_flowers_for_pharmacy(session, pharmacy, "TOKEN")

        assert session.requested_pages == [1, 2]
        assert [p["name"] for p in products] == ["Sorte A", "Sorte B"]
        for product in products:
            assert product["apotheke"] == "Adler Apotheke"
            assert product["apotheke_plz"] == "10115"
            assert product["apotheke_stadt"] == "Berlin"
            assert "deliveryTarget=TOKEN" in product["produkt_url"]

    def test_stops_when_a_page_has_no_tiles(self):
        session = _StubSession(["<html><body>leer</body></html>"])
        pharmacy = {"name": "Apo", "plz": "1", "stadt": "X"}
        assert scraper.scrape_flowers_for_pharmacy(session, pharmacy, "T") == []
        assert session.requested_pages == [1]
