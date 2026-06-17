#!/usr/bin/env python
# -*- coding: utf-8 -*-
# @Author: leeyoshinari

import os
import sys
import configparser
import logging.handlers

if hasattr(sys, 'frozen'):
    path = os.path.dirname(sys.executable)
else:
    path = os.path.dirname(os.path.abspath(__file__))
cfg = configparser.ConfigParser()
config_path = os.path.join(path, 'config.conf')
cfg.read(config_path, encoding='utf-8')
PAGE_SIZE = 20


def get_config(key):
    return cfg.get('default', key, fallback=None)


PREPARE_MAX_CONCURRENT = max(1, int(get_config("prepare_max_concurrent") or 2))


def get_config_bool(key, default=False):
    val = get_config(key)
    if val is None:
        return default
    return str(val).lower() in ('1', 'true', 'yes', 'on')


FILE_PATH = get_config("path")
if not FILE_PATH:
    raise FileNotFoundError("config path is required")
if not os.path.exists(FILE_PATH):
    raise FileNotFoundError(FILE_PATH)

KEEP_PATH = get_config("keep_path") or os.path.join(FILE_PATH, "__keep__")
OVERRIDE_PATH = get_config("override_path") or os.path.join(FILE_PATH, "__override__")
KEEP_DIR_NAME = get_config("keep_dir_name") or "__keep__"
OVERRIDE_DIR_NAME = get_config("override_dir_name") or "__override__"
SCAN_VIDEO_EXTS = [
    ext.strip().lower().lstrip('.')
    for ext in (get_config("scan_video_exts") or "mp4").split(',')
    if ext.strip()
]
FFPROBE_ON_IMPORT = get_config_bool("ffprobe_on_import", True)
DEFAULT_DUPLICATE_POLICY = get_config("default_duplicate_policy") or "skip"

os.makedirs(KEEP_PATH, exist_ok=True)
os.makedirs(OVERRIDE_PATH, exist_ok=True)
PLAY_CACHE_PATH = os.path.join(FILE_PATH, '__play_cache__')
os.makedirs(PLAY_CACHE_PATH, exist_ok=True)

TORTOISE_ORM = {
    "connections": {"default": f"sqlite://{FILE_PATH}/sqlite3.db"},
    "apps": {
        "models": {
            "models": ["karaoke.infra.models", "aerich.models"],
            "default_connection": "default"
        }
    },
    "timezone": "Asia/Shanghai"
}

CONTENT_TYPE = {
    'mp4': 'video/mp4',
    'mkv': 'video/x-matroska',
    'avi': 'video/x-msvideo',
    'mov': 'video/quicktime',
    'webm': 'video/webm',
    'm4v': 'video/x-m4v',
    'mp3': 'audio/mpeg',
    'm4a': 'audio/mp4',
    'aac': 'audio/aac',
    'wav': 'audio/wav',
}

log_path = os.path.join(path, 'logs')
if not os.path.exists(log_path):
    os.mkdir(log_path)

log_level = {
    'DEBUG': logging.DEBUG,
    'INFO': logging.INFO,
    'WARNING': logging.WARNING,
    'ERROR': logging.ERROR,
    'CRITICAL': logging.CRITICAL
}

logger = logging.getLogger()
formatter = logging.Formatter('%(asctime)s - %(levelname)s - %(threadName)s - %(filename)s[line:%(lineno)d] - %(message)s')
logger.setLevel(level=log_level.get(get_config("level")))

file_handler = logging.handlers.TimedRotatingFileHandler(os.path.join(log_path, 'access.log'), when='midnight', interval=1, backupCount=7)
file_handler.suffix = '%Y-%m-%d'
file_handler.setFormatter(formatter)
logger.addHandler(file_handler)
