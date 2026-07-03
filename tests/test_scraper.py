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
