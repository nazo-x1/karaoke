const server = localStorage.getItem("server");
const songId = window.SONG_ID;

function renderMeta(data) {
    const mode = data.playback_mode === 'enhanced' ? '增强' : (data.can_queue ? '仅播放' : '不可用');
    const of = data.override_files || {};
    document.getElementById("song-meta").innerHTML =
        `来源：${data.source_origin === 'upload' ? '上传' : '扫描'}<br>` +
        `播放模式：${mode}<br>` +
        `可点歌：${data.can_queue ? '是' : '否'}<br>` +
        `覆写文件：视频 ${of.video ? '✓' : '×'} / 人声 ${of.vocals ? '✓' : '×'} / 伴奏 ${of.accompaniment ? '✓' : '×'}`;
}

function loadSong() {
    $.ajax({
        type: "GET",
        url: server + "/song/" + songId,
        success: function (data) {
            if (data.code !== 0) {
                $.Toast(data.msg, "error");
                return;
            }
            document.getElementById("display-name").value = data.data.display_name;
            document.getElementById("source-path").value = data.data.source_path;
            renderMeta(data.data);
        }
    });
}

document.getElementById("save-btn").addEventListener("click", () => {
    const display_name = document.getElementById("display-name").value.trim();
    $.ajax({
        type: "PATCH",
        url: server + "/song/" + songId,
        contentType: "application/json",
        data: JSON.stringify({ display_name }),
        success: function (data) {
            if (data.code === 0) {
                $.Toast("保存成功", "success");
                loadSong();
            } else {
                $.Toast(data.msg, "error");
            }
        }
    });
});

document.getElementById("detect-btn").addEventListener("click", () => {
    $.ajax({
        type: "POST",
        url: server + "/song/" + songId + "/detect-override",
        success: function (data) {
            if (data.code === 0) {
                $.Toast(data.msg, "success");
                loadSong();
            } else {
                $.Toast(data.msg, "error");
            }
        }
    });
});

window.onload = loadSong;
