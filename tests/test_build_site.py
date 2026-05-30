"""Unit tests for the parsing/derivation helpers in build_site."""

import textwrap

import pytest

import build_site

CSV_HEADER = (
    "apotheke,apotheke_plz,apotheke_stadt,name,bezeichnung,genetik,"
    "thc,cbd,preis_pro_gramm,verfuegbarkeit"
)


def _write_csv(tmp_path, rows: str):
    path = tmp_path / "flowers.csv"
    path.write_text(CSV_HEADER + "\n" + textwrap.dedent(rows).strip() + "\n", encoding="utf-8")
    return path


class TestParseDecimal:
    @pytest.mark.parametrize(
        "value,expected",
        [
            ("9,50 €", 9.5),
            ("8.00", 8.0),
            ("12", 12.0),
            ("1\xa0234,5", 1.0),  # first number wins; nbsp normalised to space
            ("ab 7,25 €/g", 7.25),
        ],
    )
    def test_parses_first_number(self, value, expected):
        assert build_site.parse_decimal(value) == expected

    @pytest.mark.parametrize("value", ["", "   ", "kein Preis", "€"])
    def test_returns_none_without_number(self, value):
        assert build_site.parse_decimal(value) is None


class TestParsePercent:
    @pytest.mark.parametrize(
        "value,expected",
        [
            ("20%", 20.0),
            ("18,5 %", 18.5),
            ("1", 1.0),
        ],
    )
    def test_plain_percent(self, value, expected):
        assert build_site.parse_percent(value) == expected

    def test_less_than_prefix_subtracts_epsilon(self):
        # "<1%" should be treated as just under 1.
        assert build_site.parse_percent("<1%") == pytest.approx(0.99)

    def test_less_than_never_negative(self):
        assert build_site.parse_percent("<0%") == 0

    @pytest.mark.parametrize("value", ["", "   ", "n/a"])
    def test_returns_none_without_number(self, value):
        assert build_site.parse_percent(value) is None


class TestCalculateThcPrice:
    def test_basic_division(self):
        # 9.50 €/g at 20% THC -> 47.50 €/g THC
        assert build_site.calculate_thc_price(9.5, 20.0) == 47.5

    def test_rounds_to_two_decimals(self):
        assert build_site.calculate_thc_price(8.0, 18.0) == 44.44

    @pytest.mark.parametrize(
        "price,thc",
        [
            (None, 20.0),
            (9.5, None),
            (9.5, 0),
            (9.5, -5),
        ],
    )
    def test_returns_none_for_invalid_inputs(self, price, thc):
        assert build_site.calculate_thc_price(price, thc) is None


class TestCleanText:
    @pytest.mark.parametrize(
        "value,expected",
        [
            ("  hello   world ", "hello world"),
            ("non\xa0breaking", "non breaking"),
            (None, ""),
            ("", ""),
            ("\t\n  spaced \n", "spaced"),
        ],
    )
    def test_normalises_whitespace(self, value, expected):
        assert build_site.clean_text(value) == expected


class TestGroupByStrain:
    def test_same_strain_across_pharmacies_is_deduplicated(self, tmp_path):
        path = _write_csv(
            tmp_path,
            """
            Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
            Apo B,20095,Hamburg,Sorte X,EMK,Indica,20%,1%,"8,00 €",neu
            Apo C,50667,Köln,Sorte Y,XYZ,Sativa,18%,1%,"7,00 €",verfügbar
            """,
        )
        strains = build_site.group_by_strain(build_site.read_offers(path))

        assert len(strains) == 2
        by_name = {s["name"]: s for s in strains}

        x = by_name["Sorte X"]
        assert x["pharmacy_count"] == 2
        assert len(x["offers"]) == 2
        # cheapest offer first
        assert [o["apotheke"] for o in x["offers"]] == ["Apo B", "Apo A"]
        assert x["min_price"] == 8.0
        # 8.00 / (20/100) = 40.00 €/g THC
        assert x["min_price_per_thc_gram"] == 40.0

    def test_grouping_is_case_insensitive(self, tmp_path):
        path = _write_csv(
            tmp_path,
            """
            Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
            Apo B,20095,Hamburg,sorte x,emk,Indica,20%,1%,"8,00 €",neu
            """,
        )
        strains = build_site.group_by_strain(build_site.read_offers(path))
        assert len(strains) == 1
        assert strains[0]["pharmacy_count"] == 2

    def test_search_index_includes_pharmacies(self, tmp_path):
        path = _write_csv(
            tmp_path,
            """
            Adler Apotheke,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
            """,
        )
        strain = build_site.group_by_strain(build_site.read_offers(path))[0]
        assert "adler apotheke" in strain["search"]
        assert "berlin" in strain["search"]

    def test_metadata_counts_offers_and_strains(self, tmp_path):
        path = _write_csv(
            tmp_path,
            """
            Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,1%,"9,50 €",verfügbar
            Apo B,20095,Hamburg,Sorte X,EMK,Indica,20%,1%,"8,00 €",neu
            """,
        )
        offers = build_site.read_offers(path)
        strains = build_site.group_by_strain(offers)
        metadata = build_site.build_metadata(offers, strains)
        assert metadata["total"] == 2
        assert metadata["strain_count"] == 1
        assert metadata["pharmacy_count"] == 2
        assert metadata["lowest_price"] == 8.0


class TestHighlights:
    def _metadata(self, tmp_path):
        path = _write_csv(
            tmp_path,
            """
            Apo A,10115,Berlin,Sorte X,EMK,Indica,20%,2%,"10,00 €",verfügbar
            Apo B,20095,Hamburg,Sorte Y,XYZ,Sativa,30%,1%,"9,00 €",neu
            Apo C,50667,Köln,Sorte Z,ABC,Hybrid,15%,8%,"6,00 €",verfügbar
            """,
        )
        offers = build_site.read_offers(path)
        return build_site.build_metadata(offers, build_site.group_by_strain(offers))

    def test_cheapest_per_gram_carries_name_and_pharmacy(self, tmp_path):
        entry = self._metadata(tmp_path)["cheapest_gram"]
        assert (entry["price"], entry["name"], entry["apotheke"]) == (6.0, "Sorte Z", "Apo C")

    def test_cheapest_per_gram_thc(self, tmp_path):
        # 9.00 / 0.30 = 30.00 €/g THC is cheapest
        entry = self._metadata(tmp_path)["cheapest_thc_gram"]
        assert (entry["price"], entry["name"], entry["apotheke"]) == (30.0, "Sorte Y", "Apo B")

    def test_cheapest_per_gram_cbd(self, tmp_path):
        # 6.00 / 0.08 = 75.00 €/g CBD is cheapest
        entry = self._metadata(tmp_path)["cheapest_cbd_gram"]
        assert (entry["price"], entry["name"], entry["apotheke"]) == (75.0, "Sorte Z", "Apo C")

    def test_highest_thc(self, tmp_path):
        entry = self._metadata(tmp_path)["highest_thc"]
        assert (entry["name"], entry["apotheke"], entry["thc"]) == ("Sorte Y", "Apo B", "30%")

    def test_highest_cbd(self, tmp_path):
        entry = self._metadata(tmp_path)["highest_cbd"]
        assert (entry["name"], entry["apotheke"], entry["cbd"]) == ("Sorte Z", "Apo C", "8%")

    def test_highest_thc_and_cbd_combined(self, tmp_path):
        # thc+cbd: X=22, Y=31, Z=23 -> Sorte Y wins
        entry = self._metadata(tmp_path)["highest_thc_cbd"]
        assert (entry["name"], entry["thc"], entry["cbd"]) == ("Sorte Y", "30%", "1%")
