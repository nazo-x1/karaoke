const server = KTV.config.server;
let angle = 0;
let vocalsVolume = localStorage.getItem("vocalsVolume")? parseFloat(localStorage.getItem("vocalsVolume")): 1;
let accompanimentVolume = localStorage.getItem("accompanimentVolume")? parseFloat(localStorage.getItem("accompanimentVolume")): 1;
let isBindEvent = false;
let searchTimeout = null;
let searchState = { page: 1, totalPage: 0, keyword: '', loading: false };
let currentPlaybackMode = 'plain';
const l_body = document.querySelector('.l_body');
const sidebar = {
  leftbar: () => {
    if (l_body) {
      l_body.toggleAttribute('leftbar');
      l_body.removeAttribute('rightbar');
    }
  },
  rightbar: () => {
    if (l_body) {
      l_body.toggleAttribute('rightbar');
      l_body.removeAttribute('leftbar');
    }
  },
  dismiss: () => {
    if (l_body) {
      l_body.removeAttribute('leftbar');
      l_body.removeAttribute('rightbar');
    }
  }
};

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

function updateEnhancedControlsFromQueue(data) {
    if (!data || data.total < 1) {
        currentPlaybackMode = 'plain';
        setEnhancedControlsVisible(false);
        return;
    }
    const playing = data.data.find(item => KTV.queue.isPlaying(item)) || data.data[0];
    currentPlaybackMode = playing.playback_mode === 'enhanced' ? 'enhanced' : 'plain';
    setEnhancedControlsVisible(currentPlaybackMode === 'enhanced');
}

document.getElementById("rotate-img").addEventListener('click', () => {
    if(document.getElementById("main-image").classList.contains('rotating')) {
        send_message(1, 0);
    } else {
        if (document.getElementsByClassName("current-song")[0].innerText === "暂未开始播放") {
            send_message(1, 5);
        } else {
            send_message(1, 1);
        }
    }
});

document.getElementById("re-sing").addEventListener('click', () => {send_message(2, 0);});
document.getElementById("next-song").addEventListener('click', () => {send_message(3, 0);});
document.getElementById("switchVocal").addEventListener('click', () => {
    if (currentPlaybackMode !== 'enhanced') return;
    let switch_button = document.getElementById("switchVocal");
    if (switch_button.getElementsByTagName('p')[0].innerText === "原唱") {
        send_message(4, 1);
    } else {
        send_message(4, 0);
    }
});
document.getElementById("guzhang").addEventListener('click', () => {send_message(7, 'guzhang');});
document.getElementById("huanhu").addEventListener('click', () => {send_message(7, 'huanhu');});
document.getElementById("daxiao").addEventListener('click', () => {send_message(7, 'daxiao');});
document.getElementById("xixu").addEventListener('click', () => {send_message(7, 'xixu');});
function renderSearchSongItem(item) {
    const link = item.can_queue
        ? `<a onclick="sing_song(${item.id})">点歌</a>`
        : `<span style="color:#999">不可用</span>`;
    return `<div class="song-list"><div>${item.display_name}</div>${link}</div>`;
}

function loadSearchSongs(reset) {
    if (searchState.loading) return;
    const keyWord = document.getElementById("search-text").value.trim();
    if (!keyWord) {
        document.getElementsByClassName("song-container")[0].innerHTML = '';
        searchState = { page: 1, totalPage: 0, keyword: '', loading: false };
        return;
    }
    if (reset) {
        searchState.page = 1;
        searchState.totalPage = 0;
        searchState.keyword = keyWord;
        document.getElementsByClassName("song-container")[0].innerHTML = '';
    } else if (searchState.page > searchState.totalPage && searchState.totalPage > 0) {
        return;
    }
    const pageToLoad = searchState.page;
    searchState.loading = true;
    KTV.library.list(pageToLoad, keyWord).then(function (data) {
        searchState.totalPage = data.totalPage || 0;
        const container = document.getElementsByClassName("song-container")[0];
        (data.data || []).forEach(function (item) {
            container.insertAdjacentHTML('beforeend', renderSearchSongItem(item));
        });
        searchState.page = pageToLoad + 1;
    }).catch(function (err) {
        console.log(err.msg || err);
    }).finally(function () {
        searchState.loading = false;
    });
}

document.getElementById("search-text").addEventListener('input', () =>{
    clearTimeout(searchTimeout);
    searchTimeout = setTimeout(function () {
        loadSearchSongs(true);
    }, 500);
});

const searchScrollRoot = document.querySelector('.l_left .leftbar-container');
if (searchScrollRoot) {
    searchScrollRoot.addEventListener('scroll', function () {
        if (!searchState.keyword || searchState.loading) return;
        if (searchScrollRoot.scrollTop + searchScrollRoot.clientHeight >= searchScrollRoot.scrollHeight - 80) {
            loadSearchSongs(false);
        }
    });
}
document.getElementById("change-volume").addEventListener("click", () => {
    if (currentPlaybackMode !== 'enhanced') return;
    let volume_setting = document.getElementsByClassName("volume-setting")[0];
    if (volume_setting.style.display === 'flex') {
        volume_setting.style.display = 'none';
        return;
    }
    volume_setting.style.display = 'flex';
    initVolume("volume-vocals-progress");
    initVolume("volume-acc-progress");
});

startRotate = () => {
    if(!document.getElementById("main-image").classList.contains('rotating')) {
        document.getElementById("main-image").classList.add('rotating');
    }
    document.getElementById("rotate-img").getElementsByTagName("img")[0].src = "/static/img/stopRotate.svg";
    document.getElementById("rotate-img").getElementsByTagName("p")[0].innerText = "暂停";
    document.getElementById("main-image").style.transform = `rotate(${angle}deg)`;
};

stopRotate = () => {
    document.getElementById("rotate-img").getElementsByTagName("img")[0].src = "/static/img/startRotate.svg";
    document.getElementById("rotate-img").getElementsByTagName("p")[0].innerText = "开始";
    const image = document.getElementById("main-image");
    const computedStyle = getComputedStyle(image);
    const matrix = new WebKitCSSMatrix(computedStyle.transform);
    angle = Math.round(Math.atan2(matrix.m21, matrix.m11) * (180 / Math.PI));
    image.classList.remove('rotating');
    image.style.transform = `rotate(${angle}deg)`;
};

send_message = (code, data) => {
    KTV.events.send(code, data).catch(function (err) {
        console.log(err.msg);
    });
};

getSingList = () => {
    KTV.queue.pending().then(function (data) {
        updateEnhancedControlsFromQueue(data);
        if (data.total > 0) {
            if (KTV.queue.isPlaying(data.data[0])) {
                document.getElementsByClassName("current-song")[0].innerText = data.data[0].name;
                startRotate();
                if (data.data.length > 1) {
                    document.getElementsByClassName("next-song")[0].innerText = "下一首：" + data.data[1].name;
                } else {
                    document.getElementsByClassName("next-song")[0].innerText = "暂无下一首歌曲";
                }
            } else {
                document.getElementsByClassName("current-song")[0].innerText = "暂未开始播放";
                document.getElementsByClassName("next-song")[0].innerText = "下一首：" + data.data[0].name;
                stopRotate();
            }
        } else {
            document.getElementsByClassName("current-song")[0].innerText = "暂未开始播放";
            document.getElementsByClassName("next-song")[0].innerText = "暂无下一首歌曲";
            stopRotate();
        }
        let s = "";
        data.data.forEach(item => {
            s = s + `<div class="song-list"><div>${item.name}</div><a onclick="set_top(${item.id})">置顶</a><a onclick="delete_from_list(${item.id})">删除</a></div>`;
        });
        document.getElementsByClassName("added-container")[0].innerHTML = s;
        document.getElementById("added-song-num").innerText = data.total;
        document.getElementById("added-song-num1").innerText = data.total;
    }).catch(function (err) {
        console.log(err.msg);
    });
};

switchVocal = (flag) => {
    if (currentPlaybackMode !== 'enhanced') return;
    let switch_button = document.getElementById("switchVocal");
    if (flag === 'ON') {
        switch_button.getElementsByTagName('p')[0].innerText = "原唱";
        switch_button.style.filter = "grayscale(0)";
    } else {
        switch_button.getElementsByTagName('p')[0].innerText = "伴奏";
        switch_button.style.filter = "grayscale(1)";
    }
};

initVolume = (eleId) => {
    let mdown = false;
    let progressEle = document.getElementById(eleId);
    let startIndex = progressEle.getBoundingClientRect().left;
    if (eleId === "volume-vocals-progress") {
        progressEle.getElementsByClassName("mkpgb-cur")[0].style.width = vocalsVolume * 100 + '%';
        progressEle.getElementsByClassName("mkpgb-dot")[0].style.left = vocalsVolume * 100 + '%';
    } else {
        progressEle.getElementsByClassName("mkpgb-cur")[0].style.width = accompanimentVolume * 100 + '%';
        progressEle.getElementsByClassName("mkpgb-dot")[0].style.left = accompanimentVolume * 100 + '%';
    }
    if (isBindEvent) {return;}
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
        if (eleId === "volume-vocals-progress") {send_message(5, vocalsVolume);
        } else {send_message(6, accompanimentVolume);}
        mdown = false;
        isBindEvent = true;
    });
    function barMove(e) {
        if(!mdown) return;
        let percent = 0;
        if(e.clientX < startIndex){
            percent = 0;
        }else if(e.clientX > progressEle.clientWidth + startIndex){
            percent = 1;
        }else{
            percent = (e.clientX - startIndex) / progressEle.clientWidth;
        }
        if (eleId === "volume-vocals-progress") {
            vocalsVolume = percent;
        } else {
            accompanimentVolume = percent;
        }
        progressEle.getElementsByClassName("mkpgb-cur")[0].style.width = percent * 100 + '%';
        progressEle.getElementsByClassName("mkpgb-dot")[0].style.left = percent * 100 + '%';
        return true;
    }
};

function sing_song(file_id) {
    KTV.queue.enqueue(file_id).catch(function (err) {
        console.log(err.msg);
    });
}

function set_top(file_id) {
    KTV.queue.setTop(file_id).catch(function (err) {
        console.log(err.msg);
    });
}

function delete_from_list(file_id) {
    KTV.queue.remove(file_id).catch(function (err) {
        console.log(err.msg);
    });
}

window.onload = function() {
    setEnhancedControlsVisible(false);
    KTV.events.connect();
    KTV.events.on(1, function (message) {
        if (message.data === '4') {stopRotate();}
        if (message.data === '3') {startRotate(); getSingList();}
    });
    KTV.events.on(4, function (message) {
        if (currentPlaybackMode === 'enhanced') {
            if (message.data === '0') {switchVocal("ON");}
            if (message.data === '1') {switchVocal("OFF");}
        }
    });
    KTV.events.on(5, function (message) {
        vocalsVolume = parseFloat(message.data);
        localStorage.setItem('vocalsVolume', message.data);
    });
    KTV.events.on(6, function (message) {
        accompanimentVolume = parseFloat(message.data);
        localStorage.setItem('accompanimentVolume', message.data);
    });
    KTV.events.on(8, function () {
        getSingList();
    });
    getSingList();
};
