window.KTV = window.KTV || {};

(function (KTV) {
    var base = function (id) { return KTV.config.apiBase + '/songs/' + id; };

    KTV.songConfig = {
        detail: function (songId) {
            return KTV.http.get(base(songId));
        },
        patch: function (songId, body) {
            return KTV.http.patch(base(songId), body);
        },
        detect: function (songId) {
            return KTV.http.post(base(songId) + '/detect');
        },
        prepare: function (songId, wait) {
            var url = base(songId) + '/prepare' + (wait ? '?wait=1' : '');
            return KTV.http.post(url);
        },
    };
})(window.KTV);
