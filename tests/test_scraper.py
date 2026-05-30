"""Unit tests for scraper parsing helpers (no network access)."""

from base64 import b64decode

from bs4 import BeautifulSoup

import scraper


def _tile(html: str):
    return BeautifulSoup(html, "lxml").find("article")


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
              <h2>Test Blüte 20/1</h2>
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
        }


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
