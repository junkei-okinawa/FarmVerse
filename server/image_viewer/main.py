import os
import math
from datetime import datetime
from typing import List, Optional, Tuple
from dotenv import load_dotenv
import logging

import asyncio
import aiofiles
from fastapi import FastAPI, Request, Query, HTTPException
from fastapi.responses import HTMLResponse, FileResponse
from fastapi.templating import Jinja2Templates
from fastapi.staticfiles import StaticFiles

# --- Logging Setup ---
logging.basicConfig(level=logging.INFO, format="%(asctime)s - %(levelname)s - %(message)s")
logger = logging.getLogger(__name__)

# --- .env ファイルの読み込み ---
load_dotenv()
logger.info(".env file loaded.")

# --- 設定 ---
IMAGE_DIR = os.getenv("VIEWER_IMAGE_DIR")
if not IMAGE_DIR:
    logger.error("VIEWER_IMAGE_DIR environment variable is not set.")
    raise ValueError("VIEWER_IMAGE_DIR environment variable is not set.")
if not os.path.exists(IMAGE_DIR):
    logger.error(f"Image directory does not exist: {IMAGE_DIR}")
    raise ValueError(f"Image directory does not exist: {IMAGE_DIR}")
if not os.path.isdir(IMAGE_DIR):
    logger.error(f"VIEWER_IMAGE_DIR is not a directory: {IMAGE_DIR}")
    raise ValueError(f"VIEWER_IMAGE_DIR is not a directory: {IMAGE_DIR}")

IMAGES_PER_PAGE = 50 # 1ページあたりの画像数

logger.info(f"Using image directory: {IMAGE_DIR}")
logger.info(f"Images per page: {IMAGES_PER_PAGE}")

# --- FastAPI アプリケーションの初期化 ---
app = FastAPI(title="ESP Image Viewer")
templates = Jinja2Templates(directory="templates")

# 静的ファイル (画像) を提供するためのマウント
image_dir_exists = os.path.exists(IMAGE_DIR) and os.path.isdir(IMAGE_DIR)
if image_dir_exists:
    app.mount("/images", StaticFiles(directory=IMAGE_DIR), name="images")
    logger.info(f"Mounted static files directory '/images' to '{IMAGE_DIR}'")
else:
    logger.warning(f"Image directory not found or is not a directory: {IMAGE_DIR}")
    logger.warning("Viewer will run, but images will not be displayed.")
    logger.warning("Set the VIEWER_IMAGE_DIR environment variable or create the directory.")


# --- ヘルパー関数 ---
def parse_filename(filename: str) -> Optional[Tuple[str, datetime]]:
    """ファイル名からMACアドレスとタイムスタンプを抽出する"""
    logger.debug(f"Attempting to parse filename: {filename}")
    # 例: 34ab95fa3a6c_20250414_115521_331015.jpg
    try:
        # 拡張子を除いてから分割
        base_name = filename.lower().removesuffix('.jpg')
        parts = base_name.split("_")
        logger.debug(f"Split parts: {parts}")
        # 期待するパーツ数を確認 (MAC + Date + Time + Microseconds = 4)
        if len(parts) == 4:
            mac_part = parts[0]
            # MACアドレス形式 (12桁の16進数) か簡易チェック
            if len(mac_part) == 12 and all(c in '0123456789abcdef' for c in mac_part): # 小文字のみチェック
                 mac_addr = ":".join(mac_part[i:i+2] for i in range(0, 12, 2)) # lower() は不要に
                 logger.debug(f"Parsed MAC: {mac_addr}")
            else:
                 logger.warning(f"Skipping file due to invalid MAC format: {filename}")
                 return None

            # タイムスタンプ部分を結合 (Date_Time_Microseconds)
            timestamp_str = "_".join(parts[1:]) # 修正: parts[1], parts[2], parts[3] を結合
            logger.debug(f"Attempting to parse timestamp string: {timestamp_str}")
            # マイクロ秒まで対応
            dt_obj = datetime.strptime(timestamp_str, "%Y%m%d_%H%M%S_%f")
            logger.debug(f"Parsed datetime: {dt_obj}")
            return mac_addr, dt_obj
        else:
            logger.warning(f"Skipping file due to unexpected number of parts ({len(parts)}): {filename}") # ログメッセージ修正
            return None
    except (ValueError, IndexError) as e:
        # ValueErrorはstrptimeでも発生しうる
        logger.error(f"Could not parse filename '{filename}': {e}", exc_info=True)
        return None # return None を明示
    return None

async def list_image_files(image_dir: str) -> List[str]:
    """指定されたディレクトリ内の .jpg ファイルのリストを非同期で取得する"""
    if not image_dir_exists:
        return []
    try:
        # os.listdir はブロッキング I/O なので run_in_executor を使う
        loop = asyncio.get_running_loop()
        filenames = await loop.run_in_executor(
            None, lambda: [f for f in os.listdir(image_dir) if f.lower().endswith(".jpg")]
        )
        return filenames
    except FileNotFoundError:
        logger.error(f"listdir failed for directory: {image_dir}")
        return []
    except Exception as e:
        logger.error(f"Error listing files in {image_dir}: {e}")
        return []


async def get_image_files_details(
    image_dir: str,
    filter_mac: Optional[str] = None,
    filter_date: Optional[str] = None, # YYYY-MM-DD 形式
    sort_by: str = "timestamp", # 'timestamp' or 'mac'
    sort_order: str = "desc", # 'asc' or 'desc'
) -> Tuple[List[Tuple[str, str, datetime]], List[str]]:
    """画像ディレクトリからファイル情報を取得し、フィルタリング・ソートする。利用可能なMACリストも返す"""
    filenames = await list_image_files(image_dir)
    if not filenames:
        return [], []

    parsed_files = []
    all_macs_set = set()
    for filename in filenames:
        parsed = parse_filename(filename)
        if parsed:
            mac, dt = parsed
            parsed_files.append((filename, mac, dt))
            all_macs_set.add(mac)

    logger.info(f"Parsed {len(parsed_files)} files with MAC addresses: {all_macs_set}")
    # フィルタリング
    filtered_files = parsed_files
    if filter_mac:
        # MACアドレスは小文字で比較
        filtered_files = [f for f in filtered_files if f[1] == filter_mac.lower()]
    if filter_date:
        try:
            filter_dt = datetime.strptime(filter_date, "%Y-%m-%d").date()
            filtered_files = [f for f in filtered_files if f[2].date() == filter_dt]
        except ValueError:
            logger.warning(f"Invalid date format received: {filter_date}. Ignoring filter.")
            pass # 日付形式が不正な場合は無視

    # ソート
    reverse_order = sort_order == "desc"
    if sort_by == "mac":
        filtered_files.sort(key=lambda x: x[1], reverse=reverse_order)
    else: # デフォルトはタイムスタンプ
        filtered_files.sort(key=lambda x: x[2], reverse=reverse_order)

    available_macs = sorted(list(all_macs_set))

    return filtered_files, available_macs

# --- ルーティング ---
@app.get("/", response_class=HTMLResponse)
async def read_images(
    request: Request,
    page: int = Query(1, ge=1),
    limit: int = Query(default=IMAGES_PER_PAGE, ge=1, le=200), # 1ページあたりの表示件数
    filter_mac: Optional[str] = Query(None, description="Filter by MAC address (e.g., 0a:1b:2c:3d:4e:5f)"),
    filter_date: Optional[str] = Query(None, description="Filter by date (YYYY-MM-DD)"),
    sort_by: str = Query("timestamp", pattern="^(timestamp|mac)$", description="Sort by 'timestamp' or 'mac'"),
    sort_order: str = Query("desc", pattern="^(asc|desc)$", description="Sort order 'asc' or 'desc'"),
):
    """画像一覧を表示するメインページ"""
    error_message = None
    if not image_dir_exists:
        error_message = f"Image directory not found: {IMAGE_DIR}. Please check the path or set the VIEWER_IMAGE_DIR environment variable."
        all_files_info = []
        available_macs = []
    else:
        try:
            all_files_info, available_macs = await get_image_files_details(
                IMAGE_DIR, filter_mac, filter_date, sort_by, sort_order
            )
        except Exception as e:
            logger.error(f"Error getting image file details: {e}", exc_info=True)
            error_message = "An error occurred while retrieving image information."
            all_files_info = []
            available_macs = []


    total_images = len(all_files_info)
    total_pages = math.ceil(total_images / limit) if limit > 0 else 1
    # ページ番号が範囲外の場合、最後のページに調整（または最初のページ）
    page = max(1, min(page, total_pages)) if total_pages > 0 else 1

    start_index = (page - 1) * limit
    end_index = start_index + limit
    paginated_files = all_files_info[start_index:end_index]

    logger.info(f"Request: page={page}, limit={limit}, filter_mac={filter_mac}, filter_date={filter_date}, sort_by={sort_by}, sort_order={sort_order}")
    logger.info(f"Found {total_images} total images, returning {len(paginated_files)} for page {page}/{total_pages}")

    context = {
        "request": request,
        "images": paginated_files, # (filename, mac, datetime) のリスト
        "page": page,
        "total_pages": total_pages,
        "limit": limit,
        "filter_mac": filter_mac,
        "filter_date": filter_date,
        "sort_by": sort_by,
        "sort_order": sort_order,
        "available_macs": available_macs,
        "error_message": error_message,
        "total_images": total_images, # 総画像数をテンプレートに渡す
    }

    return templates.TemplateResponse("index.html", context)


# --- アプリケーション起動 (uvicorn/gunicornで実行) ---
# `uv run dev` または `gunicorn main:app ...` で起動するため、
# `if __name__ == "__main__":` ブロックは不要です。