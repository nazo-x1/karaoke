#!/usr/bin/env python
# -*- coding: utf-8 -*-

from fastapi import APIRouter, Request, Body

from karaoke.api.routes import library, config, queue, playback, events, system

router = APIRouter(prefix='/api/v1', tags=['KTV v1'])

# 曲库
router.add_api_route('/library/upload', library.upload_file, methods=['POST'], summary='上传歌曲')
router.add_api_route('/library/scan', library.run_scan, methods=['POST'], summary='扫描导入')
router.add_api_route('/library/scan/preview', library.preview_scan, methods=['GET'], summary='扫描预览')
router.add_api_route('/library/songs', library.get_list, methods=['GET'], summary='歌曲列表')
router.add_api_route('/library/songs/{song_id}', library.delete_song, methods=['DELETE'], summary='删除歌曲')

# 歌曲配置
router.add_api_route('/songs/{song_id}', config.get_song, methods=['GET'], summary='歌曲详情')
router.add_api_route('/songs/{song_id}', config.patch_song, methods=['PATCH'], summary='更新歌曲')
router.add_api_route('/songs/{song_id}/detect', config.detect_playback, methods=['POST'], summary='检测播放能力')
router.add_api_route('/songs/{song_id}/prepare', config.prepare_embedded, methods=['POST'], summary='预生成内嵌缓存')

# 点歌队列
router.add_api_route('/queue/songs/{song_id}', queue.enqueue, methods=['POST'], summary='点歌')
router.add_api_route('/queue', queue.list_pending, methods=['GET'], summary='待播队列')
router.add_api_route('/queue/history', queue.list_history, methods=['GET'], summary='已唱历史')
router.add_api_route('/queue/usually', queue.list_usually, methods=['GET'], summary='常点歌曲')
router.add_api_route('/queue/songs/{song_id}/top', queue.set_top, methods=['POST'], summary='置顶')
router.add_api_route('/queue/songs/{song_id}', queue.remove, methods=['DELETE'], summary='从队列移除')

# 播放
router.add_api_route('/playback/songs/{song_id}', playback.get_profile, methods=['GET'], summary='播放配置')
router.add_api_route('/playback/songs/{song_id}/prepare', playback.get_prepare, methods=['GET'], summary='准备状态')
router.add_api_route('/playback/songs/{song_id}/prepare', playback.schedule_prepare, methods=['POST'], summary='开始准备')
router.add_api_route('/playback/stream/{song_id}/{kind}', playback.stream, methods=['GET'], summary='流媒体')
router.add_api_route('/playback/session/singing/{song_id}', playback.mark_singing, methods=['POST'])
router.add_api_route('/playback/session/finished/{song_id}', playback.mark_finished, methods=['POST'])
router.add_api_route(
    '/playback/session/skip-unready/{song_id}',
    playback.skip_if_not_ready,
    methods=['POST'],
    summary='跳过未就绪歌曲',
)

# 事件
router.add_api_route('/events', events.sse_events, methods=['GET'], summary='SSE')
router.add_api_route('/events/command', events.send_command_post, methods=['POST'], summary='发送遥控指令')

# 系统（测试 / 维护）
router.add_api_route(
    '/system/play-cache/clear',
    system.clear_play_cache,
    methods=['POST'],
    summary='清除播放转码缓存',
)
