"""Tests for DataParser utility class."""

import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from utils.data_parser import DataParser


class TestDataParser:
    """DataParser utility class tests."""

    def test_extract_value_from_payload_basic(self):
        """Basic value extraction test."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5"
        
        volt_value = DataParser.extract_value_from_payload(payload, "VOLT:")
        temp_value = DataParser.extract_value_from_payload(payload, "TEMP:")
        hash_value = DataParser.extract_value_from_payload(payload, "HASH:")
        
        assert volt_value == "75"
        assert temp_value == "23.5"
        assert hash_value == "abc123"

    def test_extract_value_from_payload_not_found(self):
        """Test extraction when prefix not found."""
        payload = "HASH:abc123,VOLT:75"
        
        result = DataParser.extract_value_from_payload(payload, "TEMP:")
        assert result is None

    def test_extract_value_from_payload_direct_format(self):
        """Test extraction for direct format (without commas)."""
        payload = "VOLT:85"
        
        result = DataParser.extract_value_from_payload(payload, "VOLT:")
        assert result == "85"

    def test_parse_voltage_data_valid(self):
        """Test voltage parsing with valid data."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5"
        
        result = DataParser.parse_voltage_data(payload)
        assert result == 75.0

    def test_parse_voltage_data_not_found(self):
        """Test voltage parsing when VOLT not found."""
        payload = "HASH:abc123,TEMP:23.5"
        
        result = DataParser.parse_voltage_data(payload)
        assert result is None

    def test_parse_voltage_data_invalid_format(self):
        """Test voltage parsing with invalid numeric format."""
        payload = "HASH:abc123,VOLT:invalid,TEMP:23.5"
        
        result = DataParser.parse_voltage_data(payload)
        assert result is None

    def test_parse_temperature_data_valid(self):
        """Test temperature parsing with valid data."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5"
        
        result = DataParser.parse_temperature_data(payload)
        assert result == 23.5

    def test_parse_temperature_data_not_found(self):
        """Test temperature parsing when TEMP not found."""
        payload = "HASH:abc123,VOLT:75"
        
        result = DataParser.parse_temperature_data(payload)
        assert result is None

    def test_parse_temperature_data_invalid_format(self):
        """Test temperature parsing with invalid numeric format."""
        payload = "HASH:abc123,VOLT:75,TEMP:invalid"
        
        result = DataParser.parse_temperature_data(payload)
        assert result is None

    def test_extract_voltage_with_validation_normal(self):
        """Test voltage extraction with validation - normal case."""
        payload = "VOLT:75"
        
        result = DataParser.extract_voltage_with_validation(payload, "test:mac")
        assert result == 75.0

    def test_extract_voltage_with_validation_skip_100(self):
        """Test voltage extraction with validation - skip 100% value."""
        payload = "VOLT:100"
        
        result = DataParser.extract_voltage_with_validation(payload, "test:mac")
        assert result is None

    def test_extract_temperature_with_validation_normal(self):
        """Test temperature extraction with validation - normal case."""
        payload = "TEMP:23.5"
        
        result = DataParser.extract_temperature_with_validation(payload, "test:mac")
        assert result == 23.5

    def test_extract_temperature_with_validation_skip_invalid(self):
        """Test temperature extraction with validation - skip -999 value."""
        payload = "TEMP:-999"
        
        result = DataParser.extract_temperature_with_validation(payload, "test:mac")
        assert result is None

    def test_extract_temperature_with_validation_partial_invalid(self):
        """Test temperature extraction with validation - skip values containing -999."""
        payload = "TEMP:-999.5"
        
        result = DataParser.extract_temperature_with_validation(payload, "test:mac")
        assert result is None
