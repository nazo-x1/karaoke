#!/usr/bin/env python
# -*- coding: utf-8 -*-

import os
import shutil
import threading
import traceback
import uuid
from concurrent.futures import ThreadPoolExecutor
from dataclasses import dataclass, field
from typing import Dict, Optional

from karaoke.ffmpeg_util import probe_duration, remove_if_exists, run_ffmpeg
from karaoke.separator_client import separate_audio
from settings import FILE_PATH, logger, get_config_section

_executor = ThreadPoolExecutor(max_workers=1)
_pipeline_lock = threading.Lock()
_jobs: Dict[str, 'PipelineJob'] = {}

AUDIO_EXTS = {'mp3', 'wav', 'flac', 'aac', 'm4a', 'ogg', 'wma'}
VIDEO_EXTS = {'mp4', 'avi', 'mkv', 'mov', 'wmv', 'flv', 'webm'}


@dataclass
class PipelineJob:
    job_id: str
    song_name: str
    status: str = 'queued'
    step: str = ''
    error: str = ''
    work_dir: str = ''
    outputs: dict = field(default_factory=dict)


def _work_root() -> str:
    root = get_config_section('pipeline', 'work_path', '/work')
    os.makedirs(root, exist_ok=True)
    return root


def get_job(job_id: str) -> Optional[PipelineJob]:
    return _jobs.get(job_id)


def submit_from_upload(upload_path: str, song_name: str) -> str:
    job_id = str(uuid.uuid4())
    work_dir = os.path.join(_work_root(), job_id)
    os.makedirs(work_dir, exist_ok=True)
    job = PipelineJob(job_id=job_id, song_name=song_name, work_dir=work_dir)
    _jobs[job_id] = job
    _executor.submit(_run_locked, job, upload_path, from_library=False)
    return job_id


def submit_from_library(song_name: str) -> str:
    src_mp4 = os.path.join(FILE_PATH, f'{song_name}.mp4')
    if not os.path.isfile(src_mp4):
        raise FileNotFoundError(f'曲库中不存在 {song_name}.mp4')
    job_id = str(uuid.uuid4())
    work_dir = os.path.join(_work_root(), job_id)
    os.makedirs(work_dir, exist_ok=True)
    shutil.copy2(src_mp4, os.path.join(work_dir, 'input.mp4'))
    job = PipelineJob(job_id=job_id, song_name=song_name, work_dir=work_dir)
    _jobs[job_id] = job
    _executor.submit(_run_locked, job, os.path.join(work_dir, 'input.mp4'), from_library=True)
    return job_id


def import_job_to_library(job_id: str) -> dict:
    job = _jobs.get(job_id)
    if not job or job.status != 'done':
        raise RuntimeError('任务未完成或不存在')
    name = job.song_name
    copied = []
    for key, filename in job.outputs.items():
        src = os.path.join(job.work_dir, filename)
        if not os.path.isfile(src):
            continue
        dst = os.path.join(FILE_PATH, filename)
        shutil.copy2(src, dst)
        copied.append(filename)
    return {'name': name, 'files': copied}


def _run_locked(job: PipelineJob, input_path: str, from_library: bool):
    with _pipeline_lock:
        _run_pipeline(job, input_path, from_library)


def _run_pipeline(job: PipelineJob, input_path: str, from_library: bool):
    job.status = 'running'
    name = job.song_name
    work = job.work_dir
    try:
        ext = input_path.rsplit('.', 1)[-1].lower()
        normalized_mp4 = os.path.join(work, 'normalized.mp4')

        if ext in AUDIO_EXTS:
            job.step = '生成占位视频'
            _make_placeholder_video(input_path, normalized_mp4)
        elif ext in VIDEO_EXTS:
            job.step = '转码为 MP4'
            if ext == 'mp4':
                shutil.copy2(input_path, normalized_mp4)
            else:
                run_ffmpeg(['-i', input_path, '-c:v', 'libx264', '-c:a', 'aac', normalized_mp4])
        else:
            raise RuntimeError(f'不支持的文件格式: {ext}')

        wav_path = os.path.join(work, 'source.wav')
        job.step = '提取音频'
        remove_if_exists(wav_path)
        run_ffmpeg(['-i', normalized_mp4, '-q:a', '0', '-map', 'a', wav_path])

        sep_dir = os.path.join(work, 'separated')
        os.makedirs(sep_dir, exist_ok=True)
        job.step = '人声伴奏分离'
        sep = separate_audio(wav_path, sep_dir, name)

        vocals_mp3 = f'{name}_vocals.mp3'
        accompaniment_mp3 = f'{name}_accompaniment.mp3'
        job.step = '转 MP3'
        run_ffmpeg(['-i', sep['vocals'], '-codec:a', 'libmp3lame', os.path.join(work, vocals_mp3)])
        run_ffmpeg(['-i', sep['accompaniment'], '-codec:a', 'libmp3lame', os.path.join(work, accompaniment_mp3)])

        silent_mp4 = os.path.join(work, f'{name}_voice.mp4')
        final_mp4 = os.path.join(work, f'{name}.mp4')
        job.step = '去音轨并优化'
        run_ffmpeg(['-i', normalized_mp4, '-an', '-vcodec', 'copy', silent_mp4])
        run_ffmpeg([
            '-i', silent_mp4, '-map_metadata', '0', '-c:v', 'copy', '-c:a', 'copy',
            '-movflags', '+faststart', final_mp4,
        ])

        job.outputs = {
            'video': f'{name}.mp4',
            'vocals': vocals_mp3,
            'accompaniment': accompaniment_mp3,
        }
        job.step = '完成'
        job.status = 'done'
        logger.info('Pipeline %s done for %s', job.job_id, name)
    except Exception as e:
        job.status = 'failed'
        job.error = str(e)
        logger.error(traceback.format_exc())


def _make_placeholder_video(audio_path: str, output_mp4: str):
    duration = probe_duration(audio_path)
    silent = os.path.join(os.path.dirname(output_mp4), '_silent.mp4')
    run_ffmpeg([
        '-f', 'lavfi', '-i', f'color=c=black:s=1280x720:d={duration}',
        '-f', 'lavfi', '-i', f'anullsrc=r=44100:cl=stereo:d={duration}',
        '-c:v', 'libx264', '-c:a', 'aac', '-shortest', silent,
    ])
    run_ffmpeg(['-i', silent, '-i', audio_path, '-c:v', 'copy', '-map', '0:v:0', '-map', '1:a:0', output_mp4])
    remove_if_exists(silent)
