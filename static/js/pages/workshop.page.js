let sessionId = sessionStorage.getItem('workshop_session_id') || '';
let pollTimer = null;
let separatorEnabled = false;

function busy(on) {
    const el = document.getElementById('busy');
    if (on) el.classList.add('show');
    else el.classList.remove('show');
}

function statusText(s) {
    const map = {
        idle: '空闲',
        preflighted: '已预检',
        assembling: '组装中',
        ai_queued: 'AI 排队中',
        ai_running: 'AI 处理中',
        ai_failed: 'AI 失败',
        product_ready: '成品就绪',
        committed: '已入库',
    };
    return map[s] || s || '-';
}

function renderSession(data) {
    if (!data) {
        document.getElementById('session-status').innerHTML = '尚未创建会话';
        return;
    }
    sessionId = data.session_id;
    sessionStorage.setItem('workshop_session_id', sessionId);
    let html = `会话：<strong>${data.session_id}</strong><br>` +
        `状态：<strong>${statusText(data.status)}</strong><br>` +
        `成品就绪：${data.product_ready ? '是' : '否'}`;
    if (data.product_filename) html += `<br>成品：${data.product_filename}`;
    if (data.display_name) html += `<br>名称：${data.display_name}`;
    if (data.ai_error) html += `<br><span style="color:#c62828">错误：${data.ai_error}</span>`;
    if (data.preflight) {
        html += `<br>预检：${data.preflight.playable ? '可播' : '不可播'} / ${data.preflight.mode_hint}`;
        if (data.preflight.suggestion) html += `<br>${data.preflight.suggestion}`;
    }
    document.getElementById('session-status').innerHTML = html;

    const aiBusy = data.status === 'ai_queued' || data.status === 'ai_running';
    if (aiBusy) startPoll();
    else stopPoll();
}

function ensureSession() {
    if (!sessionId) {
        $.Toast('请先创建会话', 'error');
        return false;
    }
    return true;
}

function startPoll() {
    if (pollTimer) return;
    pollTimer = setInterval(function () {
        if (!sessionId) return;
        KTV.workshop.getSession(sessionId).then(function (res) {
            renderSession(res.data);
        }).catch(function () {});
    }, 2000);
}

function stopPoll() {
    if (pollTimer) {
        clearInterval(pollTimer);
        pollTimer = null;
    }
}

function applyFeatures(data) {
    separatorEnabled = !!(data && data.separator_enabled);
    const card = document.getElementById('ai-card');
    if (!separatorEnabled) {
        card.classList.add('hidden');
    } else {
        card.classList.remove('hidden');
        document.getElementById('ai-hint').textContent =
            '已启用 Separator：上传普通视频 → 抽混音 → 分轨 → remux 临时双轨成品。';
    }
}

document.getElementById('btn-create').addEventListener('click', function () {
    busy(true);
    KTV.workshop.createSession().then(function (res) {
        busy(false);
        $.Toast(res.msg || '会话已创建', 'success');
        renderSession(res.data);
    }).catch(function (err) {
        busy(false);
        $.Toast(err.msg || '创建失败', 'error');
    });
});

document.getElementById('btn-refresh').addEventListener('click', function () {
    if (!ensureSession()) return;
    KTV.workshop.getSession(sessionId).then(function (res) {
        renderSession(res.data);
    }).catch(function (err) {
        $.Toast(err.msg || '刷新失败', 'error');
        sessionId = '';
        sessionStorage.removeItem('workshop_session_id');
        renderSession(null);
    });
});

document.getElementById('btn-discard').addEventListener('click', function () {
    if (!sessionId) {
        renderSession(null);
        return;
    }
    busy(true);
    KTV.workshop.destroySession(sessionId).then(function (res) {
        busy(false);
        $.Toast(res.msg || '已放弃', 'success');
        sessionId = '';
        sessionStorage.removeItem('workshop_session_id');
        stopPoll();
        renderSession(null);
    }).catch(function (err) {
        busy(false);
        $.Toast(err.msg || '删除失败', 'error');
    });
});

document.getElementById('btn-preflight').addEventListener('click', function () {
    if (!ensureSession()) return;
    const file = document.getElementById('preflight-file').files[0];
    if (!file) {
        $.Toast('请选择视频', 'error');
        return;
    }
    const fd = new FormData();
    fd.append('file', file);
    busy(true);
    KTV.workshop.preflight(sessionId, fd).then(function (res) {
        busy(false);
        $.Toast(res.msg || '预检完成', 'success');
        const pf = res.data && res.data.preflight;
        const box = document.getElementById('preflight-result');
        if (pf) {
            box.textContent = `${pf.playable ? '可播' : '不可播'} · ${pf.mode_hint} — ${pf.suggestion || ''}`;
        }
        if (res.data && res.data.session) renderSession(res.data.session);
    }).catch(function (err) {
        busy(false);
        $.Toast(err.msg || '预检失败', 'error');
    });
});

document.getElementById('btn-assemble').addEventListener('click', function () {
    if (!ensureSession()) return;
    const video = document.getElementById('assemble-video').files[0];
    const vocals = document.getElementById('assemble-vocals').files[0];
    const accomp = document.getElementById('assemble-accomp').files[0];
    if (!video || !vocals || !accomp) {
        $.Toast('请选择视频、原唱与伴奏', 'error');
        return;
    }
    const fd = new FormData();
    fd.append('video', video);
    fd.append('vocals', vocals);
    fd.append('accompaniment', accomp);
    busy(true);
    KTV.workshop.assemble(sessionId, fd).then(function (res) {
        busy(false);
        $.Toast(res.msg || '组装完成', 'success');
        renderSession(res.data);
    }).catch(function (err) {
        busy(false);
        $.Toast(err.msg || '组装失败', 'error');
    });
});

document.getElementById('btn-ai').addEventListener('click', function () {
    if (!ensureSession()) return;
    if (!separatorEnabled) {
        $.Toast('未启用 Separator', 'error');
        return;
    }
    const file = document.getElementById('ai-video').files[0];
    if (!file) {
        $.Toast('请选择视频', 'error');
        return;
    }
    const fd = new FormData();
    fd.append('file', file);
    busy(true);
    KTV.workshop.aiSeparate(sessionId, fd).then(function (res) {
        busy(false);
        $.Toast(res.msg || '已提交', 'success');
        renderSession(res.data);
        startPoll();
    }).catch(function (err) {
        busy(false);
        $.Toast(err.msg || '提交失败', 'error');
    });
});

document.getElementById('btn-commit').addEventListener('click', function () {
    if (!ensureSession()) return;
    const policy = document.getElementById('commit-policy').value;
    busy(true);
    KTV.workshop.commit(sessionId, policy).then(function (res) {
        busy(false);
        $.Toast(res.msg || '入库成功', 'success');
        sessionId = '';
        sessionStorage.removeItem('workshop_session_id');
        stopPoll();
        renderSession(null);
        setTimeout(function () { window.location.href = (localStorage.getItem('server') || '') + '/'; }, 600);
    }).catch(function (err) {
        busy(false);
        $.Toast(err.msg || '入库失败', 'error');
    });
});

window.onload = function () {
    KTV.workshop.features().then(function (res) {
        applyFeatures(res.data);
    }).catch(function () {
        applyFeatures({ separator_enabled: false });
    });
    if (sessionId) {
        KTV.workshop.getSession(sessionId).then(function (res) {
            renderSession(res.data);
        }).catch(function () {
            sessionId = '';
            sessionStorage.removeItem('workshop_session_id');
            renderSession(null);
        });
    }
};
