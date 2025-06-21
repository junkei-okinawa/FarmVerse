"""Protocol constants for frame processing."""

# Frame field sizes
MAC_ADDRESS_LENGTH = 6
FRAME_TYPE_LENGTH = 1
SEQUENCE_NUM_LENGTH = 4
LENGTH_FIELD_BYTES = 4
CHECKSUM_LENGTH = 4

# Frame markers (4 bytes each)
START_MARKER = b"\xfa\xce\xaa\xbb"
END_MARKER = b"\xcd\xef\x56\x78"

# Frame type definitions
FRAME_TYPE_HASH = 1
FRAME_TYPE_DATA = 2
FRAME_TYPE_EOF = 3

# Calculated frame lengths
HEADER_LENGTH = len(START_MARKER) + MAC_ADDRESS_LENGTH + FRAME_TYPE_LENGTH + SEQUENCE_NUM_LENGTH + LENGTH_FIELD_BYTES
FOOTER_LENGTH = CHECKSUM_LENGTH + len(END_MARKER)
