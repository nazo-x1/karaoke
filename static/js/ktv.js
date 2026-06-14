const video = document.getElementById('myVideo');
const server = localStorage.getItem("server");

let audioCtx = null;
let vocalsGain = null;
let accompanimentGain = null;
let vocalsBuffer = null;
let accompanimentBuffer = null;
let vocalsSource = null;
let accompanimentSource = null;
let audioStartTime = 0;
let audioOffset = 0;
let isAudioPlaying = false;

let videoReady = false;
let audioReady = false;
let playbackMode = 'enhanced';
let currentSongId = null;
let vocalsVolume = localStorage.getItem("vocalsVolume") ? parseFloat(localStorage.getItem("vocalsVolume")) : 1;
let accompanimentVolume = localStorage.getItem("accompanimentVolume") ? parseFloat(localStorage.getItem("accompanimentVolume")) : 1;
let openSetting = false;
let singsList = [];
let isBindEvent = false;
let isAutoPaused = false;
let soundEffectBuffers = {};
let sfxGain = null;

localStorage.setItem('vocalsVolume', vocalsVolume.toString());
localStorage.setItem('accompanimentVolume', accompanimentVolume.toString());

function streamUrl(songId, kind) {
    return server + '/song/stream/' + songId + '/' + kind;
}

const VIDEO_MIME = {
    mp4: 'video/mp4',
    m4v: 'video/x-m4v',
    webm: 'video/webm',
    mkv: 'video/x-matroska',
    avi: 'video/x-msvideo',
    mov: 'video/quicktime',
};

function setVideoSource(url, mimeType) {
    video.pause();
    while (video.firstChild) {
        video.removeChild(video.firstChild);
    }
    video.removeAttribute('src');
    const source = document.createElement('source');
    source.src = url;
    if (mimeType) {
        source.type = mimeType;
    }
    video.appendChild(source);
    video.load();
}

function bindVideoReady(flag, onReady) {
    const onCanPlay = () => {
        videoReady = true;
        if (flag) tryPlay();
        if (onReady) onReady();
    };
    video.addEventListener('canplaythrough', onCanPlay, { once: true });
    video.addEventListener('error', () => {
        const code = video.error ? video.error.code : 0;
        const hints = {
            4: '视频格式或编码不受浏览器支持',
            2: '网络错误，无法加载视频',
            3: '视频解码失败',
        };
        $.Toast(hints[code] || '视频加载失败，请检查文件格式', 'error');
        console.error('video error', video.error);
    }, { once: true });
}

function setEnhancedControlsVisible(visible) {
    const display = visible ? '' : 'none';
    ['switchVocal', 'change-volume'].forEach(id => {
        const el = document.getElementById(id);
        if (el) el.style.display = display;
    });
    if (!visible) {
        document.getElementsByClassName("volume-setting")[0].style.display = 'none';
    }
}

getSingList = (flag = false) => {
    $.ajax({
        type: "GET",
        async: flag,
        url: server + "/song/singHistory/pendingAll",
        success: function (data) {
            if (data.code === 0) {
                singsList = data.data;
            } else {
                $.Toast(data.msg, "error");
            }
        }
    });
};

showTips = () => {
    let playinText = "暂未开始播放";
    if (singsList.length > 0) {
        if (singsList[0].is_sing === -1) {
            playinText = "当前播放：" + singsList[0].name;
            if (singsList.length > 1) {
                playinText = playinText + "，下一首：" + singsList[1].name;
            } else {
                playinText = playinText + "，暂无下一首歌曲";
            }
        } else {
            playinText = playinText + "，下一首：" + singsList[0].name;
        }
    } else {
        playinText = playinText + "，暂无下一首歌曲";
    }
    document.getElementById("playing-text").innerText = playinText;
};

document.getElementById("switchVocal").addEventListener('click', () => {
    if (playbackMode !== 'enhanced') return;
    let switch_button = document.getElementById("switchVocal");
    if (switch_button.getElementsByTagName('span')[0].innerText === "原唱") {
        send_message(4, 1);
    } else {
        send_message(4, 0);
    }
});

document.getElementById("next-song").addEventListener('click', () => { send_message(3, 0); });

send_message = (code, data) => {
    $.ajax({
        type: "GET",
        url: server + "/song/send/event?code=" + code + "&data=" + data,
        success: function (data) {
            if (data.code !== 0) {
                $.Toast(data.msg, 'error');
            }
        }
    });
};

switchVocal = (flag) => {
    if (playbackMode !== 'enhanced') return;
    let switch_button = document.getElementById("switchVocal");
    if (flag === 'ON') {
        switch_button.getElementsByTagName('span')[0].innerText = "原唱";
        switch_button.style.filter = "grayscale(0)";
        if (vocalsGain) vocalsGain.gain.value = vocalsVolume;
    } else {
        switch_button.getElementsByTagName('span')[0].innerText = "伴奏";
        switch_button.style.filter = "grayscale(1)";
        if (vocalsGain) vocalsGain.gain.value = 0;
    }
};

setSinged = (flag = false) => {
    if (singsList.length < 1) return;
    $.ajax({
        type: "GET",
        async: flag,
        url: server + "/song/setSinged/" + singsList[0].id,
        success: function (data) {
            if (data.code !== 0) {
                $.Toast(data.msg, "error");
            }
        }
    });
};

setSinging = (flag = false) => {
    if (singsList.length < 1) return;
    $.ajax({
        type: "GET",
        async: flag,
        url: server + "/song/setSinging/" + singsList[0].id,
        success: function (data) {
            if (data.code !== 0) {
                $.Toast(data.msg, "error");
            }
        }
    });
};

changeVolume = (volume, flag) => {
    if (playbackMode !== 'enhanced') return;
    if (flag === 'vocals') {
        vocalsVolume = volume;
        if (vocalsGain) vocalsGain.gain.value = volume;
        localStorage.setItem('vocalsVolume', vocalsVolume.toString());
    } else {
        accompanimentVolume = volume;
        if (accompanimentGain) accompanimentGain.gain.value = volume;
        localStorage.setItem('accompanimentVolume', accompanimentVolume.toString());
    }
};

initVolume = (eleId) => {
    let mdown = false;
    let progressEle = document.getElementById(eleId);
    let startIndex = progressEle.getBoundingClientRect().left;
    if (eleId === "volume-vocals-progress") {
        if (vocalsGain) vocalsGain.gain.value = vocalsVolume;
        progressEle.getElementsByClassName("mkpgb-cur")[0].style.width = vocalsVolume * 100 + '%';
        progressEle.getElementsByClassName("mkpgb-dot")[0].style.left = vocalsVolume * 100 + '%';
    } else {
        if (accompanimentGain) accompanimentGain.gain.value = accompanimentVolume;
        progressEle.getElementsByClassName("mkpgb-cur")[0].style.width = accompanimentVolume * 100 + '%';
        progressEle.getElementsByClassName("mkpgb-dot")[0].style.left = accompanimentVolume * 100 + '%';
    }
    if (isBindEvent) { return; }
    progressEle.getElementsByClassName("mkpgb-dot")[0].addEventListener("mousedown", (e) => {
        e.preventDefault();
    });
    progressEle.addEventListener("mousedown", (e) => {
        mdown = true;
        isBindEvent = true;
        barMove(e);
    });
    progressEle.addEventListener("mousemove", (e) => {
        barMove(e);
    });
    progressEle.addEventListener("mouseup", () => {
        if (eleId === "volume-vocals-progress") { send_message(5, vocalsVolume); }
        else { send_message(6, accompanimentVolume); }
        mdown = false;
        isBindEvent = true;
    });
    function barMove(e) {
        if (!mdown) return;
        let percent = 0;
        if (e.clientX < startIndex) {
            percent = 0;
        } else if (e.clientX > progressEle.clientWidth + startIndex) {
            percent = 1;
        } else {
            percent = (e.clientX - startIndex) / progressEle.clientWidth;
        }
        if (eleId === "volume-vocals-progress") {
            vocalsVolume = percent;
            if (vocalsGain) vocalsGain.gain.value = vocalsVolume;
            localStorage.setItem('vocalsVolume', vocalsVolume.toString());
        } else {
            accompanimentVolume = percent;
            if (accompanimentGain) accompanimentGain.gain.value = accompanimentVolume;
            localStorage.setItem('accompanimentVolume', accompanimentVolume.toString());
        }
        progressEle.getElementsByClassName("mkpgb-cur")[0].style.width = percent * 100 + '%';
        progressEle.getElementsByClassName("mkpgb-dot")[0].style.left = percent * 100 + '%';
    }
};

async function preloadSoundEffects() {
    if (!audioCtx) {
        const AC = window.AudioContext || window.webkitAudioContext;
        audioCtx = new AC();
    }
    if (!sfxGain) {
        sfxGain = audioCtx.createGain();
        sfxGain.gain.value = 0.9;
        sfxGain.connect(audioCtx.destination);
    }
    const effects = ['daxiao', 'guzhang', 'huanhu', 'xixu'];
    await Promise.all(effects.map(async (name) => {
        try {
            const res = await fetch("/static/file/" + name + ".mp3");
            const arrayBuffer = await res.arrayBuffer();
            soundEffectBuffers[name] = await audioCtx.decodeAudioData(arrayBuffer);
        } catch (e) {
            console.error(`加载音效失败: `, name, e);
        }
    }));
}

userInterruption = (flag) => {
    const buffer = soundEffectBuffers[flag];
    if (buffer && audioCtx && audioCtx.state !== 'suspended') {
        const source = audioCtx.createBufferSource();
        source.buffer = buffer;
        source.connect(sfxGain);
        source.start();
    }
};

document.getElementsByClassName("setting-img")[0].addEventListener("click", () => {
    if (openSetting) {
        openSetting = false;
        document.getElementById("expand-img").src = "/static/img/expand.svg";
        document.getElementsByClassName("setting-list")[0].style.display = 'none';
        document.getElementsByClassName("volume-setting")[0].style.display = 'none';
    } else {
        openSetting = true;
        document.getElementById("expand-img").src = "/static/img/pickup.svg";
        document.getElementsByClassName("setting-list")[0].style.display = 'flex';
    }
});

document.getElementById("change-volume").addEventListener("click", () => {
    if (playbackMode !== 'enhanced') return;
    let volume_setting = document.getElementsByClassName("volume-setting")[0];
    if (volume_setting.style.display === 'flex') {
        volume_setting.style.display = 'none';
        return;
    }
    let volume_list = document.getElementsByClassName("setting")[0].offsetTop;
    volume_setting.style.top = volume_list + 72 + "px";
    volume_setting.style.display = 'flex';
    initVolume("volume-vocals-progress");
    initVolume("volume-acc-progress");
});

function initAudioContext() {
    if (!audioCtx) {
        const AC = window.AudioContext || window.webkitAudioContext;
        audioCtx = new AC();
    }
    if (audioCtx.state === 'suspended') {
        audioCtx.resume();
    }
}

function initAudioGraph() {
    if (!vocalsGain) {
        vocalsGain = audioCtx.createGain();
        vocalsGain.connect(audioCtx.destination);
        vocalsGain.gain.value = vocalsVolume;
    }
    if (!accompanimentGain) {
        accompanimentGain = audioCtx.createGain();
        accompanimentGain.connect(audioCtx.destination);
        accompanimentGain.gain.value = accompanimentVolume;
    }
}

async function loadAudioBuffer(url) {
    const res = await fetch(url);
    const arrayBuffer = await res.arrayBuffer();
    return audioCtx.decodeAudioData(arrayBuffer);
}

function resetPlaybackState() {
    stopAudio();
    vocalsBuffer = null;
    accompanimentBuffer = null;
    videoReady = false;
    audioReady = false;
    isAudioPlaying = false;
}

async function fetchPlaybackProfile(songId) {
    return new Promise((resolve, reject) => {
        $.ajax({
            type: "GET",
            url: server + "/song/" + songId + "/playback",
            success: function (data) {
                if (data.code === 0) resolve(data.data);
                else reject(new Error(data.msg));
            },
            error: function () { reject(new Error("获取播放配置失败")); }
        });
    });
}

loadSing = async (flag = false) => {
    getSingList(false);
    if (singsList.length < 1) { return; }

    const song = singsList[0];
    currentSongId = song.id;
    resetPlaybackState();

    let profile;
    try {
        profile = await fetchPlaybackProfile(song.id);
    } catch (err) {
        $.Toast(err.message || "无法加载播放配置", "error");
        return;
    }

    if (!profile.can_queue) {
        $.Toast("当前歌曲不可播放", "error");
        return;
    }

    playbackMode = profile.mode === 'enhanced' ? 'enhanced' : 'plain';
    setEnhancedControlsVisible(playbackMode === 'enhanced');

    const videoUrl = streamUrl(song.id, 'video');
    const videoMime = profile.video_mime || VIDEO_MIME[profile.video_ext] || 'video/mp4';

    if (playbackMode === 'plain') {
        video.muted = false;
        setVideoSource(videoUrl, videoMime);
        audioReady = true;
        bindVideoReady(flag);
        showTips();
        return;
    }

    video.muted = true;
    initAudioContext();
    initAudioGraph();
    setVideoSource(videoUrl, videoMime);
    bindVideoReady(flag);

    try {
        [vocalsBuffer, accompanimentBuffer] = await Promise.all([
            loadAudioBuffer(streamUrl(song.id, 'vocals')),
            loadAudioBuffer(streamUrl(song.id, 'accompaniment'))
        ]);
        audioReady = true;
        if (flag) tryPlay();
    } catch (err) {
        console.error("音频加载失败:", err);
        $.Toast("增强模式音频加载失败", "error");
        return;
    }
    showTips();
};

function playAudio(offset = 0) {
    if (playbackMode !== 'enhanced') return;
    if (!audioCtx || audioCtx.state === 'suspended') {
        initAudioContext();
    }
    stopAudio();
    vocalsSource = audioCtx.createBufferSource();
    vocalsSource.buffer = vocalsBuffer;
    vocalsSource.connect(vocalsGain);
    accompanimentSource = audioCtx.createBufferSource();
    accompanimentSource.buffer = accompanimentBuffer;
    accompanimentSource.connect(accompanimentGain);
    audioStartTime = audioCtx.currentTime;
    audioOffset = offset;
    vocalsSource.start(audioStartTime, offset);
    accompanimentSource.start(audioStartTime, offset);
    isAudioPlaying = true;
}

function stopAudio() {
    if (vocalsSource) {
        try { vocalsSource.stop(); } catch (e) {}
        vocalsSource = null;
    }
    if (accompanimentSource) {
        try { accompanimentSource.stop(); } catch (e) {}
        accompanimentSource = null;
    }
    isAudioPlaying = false;
}

function getAudioPosition() {
    if (!isAudioPlaying) return 0;
    return audioOffset + (audioCtx.currentTime - audioStartTime);
}

tryPlay = () => {
    if (videoReady && audioReady) {
        video.play().then(() => {
            if (playbackMode === 'enhanced') playAudio(0);
        }).catch(error => {
            console.error("视频播放失败:", error);
            $.Toast("第一次请手动点击播放按钮 ~", "error");
        });
    }
};

first_play = () => {
    if (singsList.length > 0) {
        loadSing(true);
    }
};

nextSong = () => {
    if (singsList.length > 0) {
        setSinged(false);
    }
    getSingList(false);
    if (singsList.length < 1) {
        document.getElementById("playing-text").innerText = "当前没有待播放的歌曲，快去点歌吧 ~";
        while (video.firstChild) {
            video.removeChild(video.firstChild);
        }
        video.removeAttribute('src');
        video.load();
        resetPlaybackState();
        return;
    }
    loadSing(true);
};

reSing = () => {
    video.currentTime = 0;
    video.play();
    if (playbackMode === 'enhanced') {
        stopAudio();
        playAudio(0);
    }
};

video.addEventListener('pause', () => {
    if (playbackMode === 'enhanced') stopAudio();
    if (!isAutoPaused) {
        send_message(1, 4);
    }
    isAutoPaused = false;
});

video.addEventListener('play', () => {
    setSinging();
    if (playbackMode === 'enhanced' && audioReady) {
        playAudio(video.currentTime);
    }
    send_message(1, 3);
    getSingList(false);
    showTips();
});

video.addEventListener('ended', () => {
    videoReady = false;
    if (playbackMode === 'enhanced') audioReady = false;
    stopAudio();
    nextSong();
});

function startSyncLoop() {
    let driftFrames = 0;
    const DRIFT_THRESHOLD = 0.01;
    const MAX_DRIFT_FRAMES = 2;
    let lastVideoTime = 0;
    let lastAudioTime = 0;
    let videoStuckFrames = 0;
    let audioStuckFrames = 0;

    function sync() {
        if (playbackMode !== 'enhanced') {
            requestAnimationFrame(sync);
            return;
        }
        if (video.currentTime === lastVideoTime) {
            videoStuckFrames++;
        } else {
            videoStuckFrames = 0;
        }
        lastVideoTime = video.currentTime;

        const currentAudioTime = audioCtx ? audioCtx.currentTime : 0;
        if (currentAudioTime === lastAudioTime) {
            audioStuckFrames++;
        } else {
            audioStuckFrames = 0;
        }
        lastAudioTime = currentAudioTime;

        if (videoStuckFrames >= 3 || audioStuckFrames >= 3) {
            if (isAudioPlaying && !video.paused && video.currentTime > 0.1) {
                isAutoPaused = true;
                video.pause();
            }
            requestAnimationFrame(sync);
            return;
        }

        if (isAutoPaused && videoStuckFrames === 0 && audioStuckFrames === 0 && video.paused) {
            video.play().then(() => {
                if (audioReady) playAudio(video.currentTime);
            }).catch(() => {});
            requestAnimationFrame(sync);
            return;
        }

        if (isAudioPlaying && !video.paused && !video.ended) {
            const audioPos = getAudioPosition();
            const diff = Math.abs(video.currentTime - audioPos);
            if (diff > DRIFT_THRESHOLD) {
                driftFrames++;
                if (driftFrames >= MAX_DRIFT_FRAMES) {
                    stopAudio();
                    playAudio(video.currentTime);
                    driftFrames = 0;
                }
            } else {
                driftFrames = 0;
            }
        } else {
            driftFrames = 0;
        }
        requestAnimationFrame(sync);
    }
    requestAnimationFrame(sync);
}

window.onload = function() {
    loadSing();
    preloadSoundEffects();
    startSyncLoop();

    const eventSource = new EventSource(server + "/song/events");
    eventSource.onmessage = function(event) {
        const message = JSON.parse(event.data);
        switch (message.code) {
            case 1:
                if (message.data === '0') { video.pause(); }
                if (message.data === '1') { video.play(); }
                if (message.data === '5') { first_play(); }
                break;
            case 2:
                reSing();
                break;
            case 3:
                nextSong();
                break;
            case 4:
                if (playbackMode === 'enhanced') {
                    if (message.data === '0') { switchVocal("ON"); }
                    if (message.data === '1') { switchVocal("OFF"); }
                }
                break;
            case 5:
                changeVolume(message.data, "vocals");
                break;
            case 6:
                changeVolume(message.data, "accompaniment");
                break;
            case 7:
                userInterruption(message.data);
                break;
            case 8:
                getSingList(false);
                showTips();
                break;
        }
    };
    eventSource.onerror = function(event) {
        console.error("EventSource failed:", event);
        eventSource.close();
    };
};
