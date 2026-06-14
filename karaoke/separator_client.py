#!/usr/bin/env python
# -*- coding: utf-8 -*-

import json
import os
import urllib.error
import urllib.request

from settings import logger, get_config_section


def separate_audio(input_wav: str, out_dir: str, stem: str) -> dict:
    sidecar_url = get_config_section('separator', 'sidecar_url', 'http://separator:8080').rstrip('/')
    vocals_pattern = get_config_section(
        'separator', 'vocals_pattern', '{out_dir}/{stem}_vocals.wav'
    )
    accompaniment_pattern = get_config_section(
        'separator', 'accompaniment_pattern', '{out_dir}/{stem}_instrumental.wav'
    )
    payload = json.dumps({
        'input': input_wav,
        'out_dir': out_dir,
        'stem': stem,
        'vocals_pattern': vocals_pattern,
        'accompaniment_pattern': accompaniment_pattern,
    }).encode('utf-8')
    req = urllib.request.Request(
        f'{sidecar_url}/separate',
        data=payload,
        headers={'Content-Type': 'application/json'},
        method='POST',
    )
    try:
        with urllib.request.urlopen(req, timeout=3600) as resp:
            body = json.loads(resp.read().decode('utf-8'))
    except urllib.error.HTTPError as e:
        detail = e.read().decode('utf-8', errors='replace')
        logger.error('Sidecar separate failed: %s %s', e.code, detail)
        raise RuntimeError(f'人声分离失败: {detail}') from e
    except urllib.error.URLError as e:
        logger.error('Sidecar unreachable: %s', e)
        raise RuntimeError(f'无法连接分离服务: {e}') from e
    vocals = body.get('vocals')
    accompaniment = body.get('accompaniment')
    if not vocals or not accompaniment:
        raise RuntimeError('分离服务未返回人声/伴奏路径')
    if not os.path.isfile(vocals) or not os.path.isfile(accompaniment):
        raise RuntimeError('分离输出文件不存在')
    return {'vocals': vocals, 'accompaniment': accompaniment}
