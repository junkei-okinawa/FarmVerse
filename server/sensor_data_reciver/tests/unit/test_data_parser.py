"""Tests for DataParser utility class."""

import sys
import os
sys.path.append(os.path.dirname(os.path.dirname(os.path.dirname(os.path.abspath(__file__)))))

from utils.data_parser import DataParser


class TestDataParser:
    """DataParser utility class tests."""

    def test_extract_value_from_payload_basic(self):
        """Basic value extraction test."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5,TDS_VOLT:0.5"
        
        volt_value = DataParser.extract_value_from_payload(payload, "VOLT:")
        temp_value = DataParser.extract_value_from_payload(payload, "TEMP:")
        hash_value = DataParser.extract_value_from_payload(payload, "HASH:")
        tds_volt_value = DataParser.extract_value_from_payload(payload, "TDS_VOLT:")
        
        assert volt_value == "75"
        assert temp_value == "23.5"
        assert hash_value == "abc123"
        assert tds_volt_value == "0.5"

    def test_extract_value_from_payload_not_found(self):
        """Test extraction when prefix not found."""
        payload = "HASH:abc123,VOLT:75"
        
        result = DataParser.extract_value_from_payload(payload, "TEMP:")
        tds_result = DataParser.extract_value_from_payload(payload, "TDS_VOLT:")
        assert result is None
        assert tds_result is None

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

    def test_extract_voltage_with_validation_with_100(self):
        """Test voltage extraction with validation - 100% value is now valid."""
        payload = "VOLT:100"
        
        result = DataParser.extract_voltage_with_validation(payload, "test:mac")
        assert result == 100.0

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

    def test_parse_tds_voltage_data_valid(self):
        """Test TDS voltage parsing with valid data."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5,TDS_VOLT:0.5"
        
        result = DataParser.parse_tds_voltage_data(payload)
        assert result == 0.5

    def test_parse_tds_voltage_data_not_found(self):
        """Test TDS voltage parsing when TDS_VOLT not found."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5"
        
        result = DataParser.parse_tds_voltage_data(payload)
        assert result is None

    def test_parse_tds_voltage_data_invalid_format(self):
        """Test TDS voltage parsing with invalid numeric format."""
        payload = "HASH:abc123,VOLT:75,TEMP:23.5,TDS_VOLT:invalid"
        
        result = DataParser.parse_tds_voltage_data(payload)
        assert result is None

    def test_extract_tds_voltage_with_validation_normal(self):
        """Test TDS voltage extraction with validation - normal case."""
        payload = "TDS_VOLT:0.8"
        
        result = DataParser.extract_tds_voltage_with_validation(payload, "test:mac")
        assert result == 0.8

    def test_extract_tds_voltage_with_validation_zero(self):
        """Test TDS voltage extraction with validation - zero value."""
        payload = "TDS_VOLT:0.0"
        
        result = DataParser.extract_tds_voltage_with_validation(payload, "test:mac")
        assert result == 0.0

    def test_extract_tds_voltage_with_validation_not_found(self):
        """Test TDS voltage extraction with validation - not found."""
        payload = "VOLT:75,TEMP:23.5"
        
        result = DataParser.extract_tds_voltage_with_validation(payload, "test:mac")
        assert result is None

    def test_extract_tds_voltage_with_validation_invalid_format(self):
        """Test TDS voltage extraction with validation - invalid format."""
        payload = "TDS_VOLT:invalid"
        
        result = DataParser.extract_tds_voltage_with_validation(payload, "test:mac")
        assert result is None
