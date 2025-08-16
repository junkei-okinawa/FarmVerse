"""Logging configuration setup."""

import logging
from config.settings import config


def setup_logging():
    """ログ設定のセットアップ"""
    # settings.pyからログレベルを取得
    log_level = getattr(logging, config.LOG_LEVEL.upper(), logging.INFO)
    
    logging.basicConfig(
        level=log_level, 
        format="%(asctime)s - %(levelname)s - %(message)s"
    )
    return logging.getLogger(__name__)
