"""Unit tests for the parsing/derivation helpers in build_site."""

import pytest

import build_site


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
