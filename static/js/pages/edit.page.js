const songId = window.SONG_ID;

const ROLE_OPTIONS = [
    { value: 'unknown', label: '未指定' },
    { value: 'vocals', label: '原唱 (vocals)' },
    { value: 'accompaniment', label: '伴奏 (accompaniment)' },
    { value: 'full', label: '完整 (full)' },
    { value: 'ignore', label: '忽略' },
];

function sourceLabel(src) {
    const map = { override: '增强(覆写)', embedded: '增强(内嵌)', plain: '仅播放' };
    return map[src] || src;
}

function renderMeta(data) {
    const mode = data.playback_mode === 'enhanced'
        ? sourceLabel(data.playback_source)
        : (data.can_queue ? '仅播放' : '不可用');
    const of = data.override_files || {};
    const cache = data.embedded_cache_ready ? '已就绪' : '未生成';
    document.getElementById("song-meta").innerHTML =
        `来源：${data.source_origin === 'upload' ? '上传' : '扫描'}<br>` +
        `播放模式：${mode}<br>` +
        `可点歌：${data.can_queue ? '是' : '否'}<br>` +
        `覆写三件套：${data.override_complete ? '齐全（优先）' : '未齐全'}<br>` +
        `内嵌缓存：${cache}<br>` +
        `覆写文件：视频 ${of.video ? '✓' : '×'} / 人声 ${of.vocals ? '✓' : '×'} / 伴奏 ${of.accompaniment ? '✓' : '×'}`;
}

function renderTracks(audioLayout) {
    const tracks = (audioLayout && audioLayout.tracks) ? audioLayout.tracks : [];
    const tbody = document.getElementById("track-body");
    if (tracks.length === 0) {
        document.getElementById("track-table").style.display = 'none';
        document.getElementById("no-tracks").style.display = 'block';
        tbody.innerHTML = '';
        return;
    }
    document.getElementById("track-table").style.display = 'table';
    document.getElementById("no-tracks").style.display = 'none';
    let html = '';
    tracks.forEach(t => {
        const opts = ROLE_OPTIONS.map(o =>
            `<option value="${o.value}" ${t.role === o.value ? 'selected' : ''}>${o.label}</option>`
        ).join('');
        html += `<tr data-index="${t.index}">
            <td>${t.index}</td>
            <td>${t.title || '-'}</td>
            <td>${t.language || '-'}</td>
            <td>${t.codec || '-'}</td>
            <td><select class="track-role">${opts}</select></td>
        </tr>`;
    });
    tbody.innerHTML = html;
}

function collectTrackRoles() {
    const rows = document.querySelectorAll('#track-body tr');
    const tracks = [];
    rows.forEach(row => {
        tracks.push({
            index: parseInt(row.getAttribute('data-index'), 10),
            role: row.querySelector('.track-role').value,
        });
    });
    return tracks;
}

function loadSong() {
    KTV.songConfig.detail(songId).then(function (res) {
        const data = res.data;
        document.getElementById("display-name").value = data.display_name;
        document.getElementById("source-path").value = data.source_path;
        renderMeta(data);
        renderTracks(data.audio_layout);
    }).catch(function (err) {
        $.Toast(err.msg, "error");
    });
}

document.getElementById("save-btn").addEventListener("click", () => {
    const display_name = document.getElementById("display-name").value.trim();
    const audio_tracks = collectTrackRoles();
    KTV.songConfig.patch(songId, { display_name, audio_tracks }).then(function () {
        $.Toast("保存成功", "success");
        loadSong();
    }).catch(function (err) {
        $.Toast(err.msg, "error");
    });
});

document.getElementById("detect-playback-btn").addEventListener("click", () => {
    KTV.songConfig.detect(songId).then(function (res) {
        $.Toast(res.msg, "success");
        loadSong();
    }).catch(function (err) {
        $.Toast(err.msg, "error");
    });
});

document.getElementById("prepare-btn").addEventListener("click", () => {
    $.Toast("正在后台生成缓存，请稍候…", "success");
    KTV.songConfig.prepare(songId, true).then(function (res) {
        $.Toast(res.msg, res.code === 0 ? "success" : "error");
        loadSong();
    }).catch(function (err) {
        $.Toast(err.msg || "请求失败", "error");
        loadSong();
    });
});

window.onload = loadSong;
