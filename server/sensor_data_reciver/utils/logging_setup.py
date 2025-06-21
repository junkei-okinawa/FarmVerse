"""Logging configuration setup."""

import logging


def setup_logging():
    """ログ設定のセットアップ"""
    logging.basicConfig(
        level=logging.INFO, 
        format="%(asctime)s - %(levelname)s - %(message)s"
    )
    return logging.getLogger(__name__)
