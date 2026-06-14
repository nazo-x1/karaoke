window.KTV = window.KTV || {};

(function (KTV) {
    var base = function (id) { return KTV.config.apiBase + '/playback/songs/' + id; };

    KTV.playback = {
        profile: function (songId) {
            return KTV.http.get(base(songId));
        },
        prepareStatus: function (songId) {
            return KTV.http.get(base(songId) + '/prepare-status');
        },
        ensureReady: function (songId) {
            return KTV.http.post(base(songId) + '/ensure-ready');
        },
        streamUrl: function (songId, kind) {
            return KTV.config.apiBase + '/playback/stream/' + songId + '/' + kind;
        },
        markSinging: function (songId) {
            return KTV.http.post(KTV.config.apiBase + '/playback/session/singing/' + songId);
        },
        markFinished: function (songId) {
            return KTV.http.post(KTV.config.apiBase + '/playback/session/finished/' + songId);
        },
        waitUntilReady: async function (songId) {
            var prep = (await KTV.playback.ensureReady(songId)).data;
            if (prep.ready) return prep;
            var deadline = Date.now() + 3600000;
            while (Date.now() < deadline) {
                if (prep.ready) return prep;
                if (prep.status === 'failed') {
                    throw new Error(prep.error || '播放资源准备失败');
                }
                await new Promise(function (r) { setTimeout(r, 1500); });
                prep = (await KTV.playback.prepareStatus(songId)).data;
            }
            throw new Error('等待播放资源超时');
        },
    };
})(window.KTV);
