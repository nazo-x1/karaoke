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
        skipUnready: function (songId) {
            return KTV.http.post(KTV.config.apiBase + '/playback/session/skip-unready/' + songId);
        },
        waitUntilReady: async function (songId) {
            var prep = (await KTV.playback.ensureReady(songId)).data;
            if (prep.ready) return prep;

            return new Promise(function (resolve, reject) {
                var done = false;
                var deadline = Date.now() + 3600000;

                function finish(err, data) {
                    if (done) return;
                    done = true;
                    if (err) reject(err);
                    else resolve(data);
                }

                function checkStatus() {
                    return KTV.playback.prepareStatus(songId).then(function (res) {
                        var p = res.data;
                        if (p.ready) {
                            finish(null, p);
                            return true;
                        }
                        if (p.status === 'failed') {
                            finish(new Error(p.error || '播放资源准备失败'));
                            return true;
                        }
                        return false;
                    });
                }

                if (KTV.events && KTV.events.connect) {
                    KTV.events.connect();
                    KTV.events.on(9, function (message) {
                        if (String(message.data) !== String(songId)) return;
                        checkStatus();
                    });
                }

                (async function pollFallback() {
                    while (!done && Date.now() < deadline) {
                        var finished = await checkStatus();
                        if (finished || done) return;
                        await new Promise(function (r) { setTimeout(r, 1500); });
                    }
                    if (!done) finish(new Error('等待播放资源超时'));
                })();
            });
        },
        formatPrepareLabel: function (prep) {
            if (!prep) return '准备中';
            var pct = prep.progress != null ? ' ' + Math.round(prep.progress) + '%' : '';
            return (prep.message || '准备中') + pct;
        },
    };
})(window.KTV);
