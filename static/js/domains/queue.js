window.KTV = window.KTV || {};

(function (KTV) {
    var base = function () { return KTV.config.apiBase + '/queue'; };

    KTV.queue = {
        enqueue: function (songId) {
            return KTV.http.post(base() + '/songs/' + songId);
        },
        pending: function () {
            return KTV.http.get(base());
        },
        history: function () {
            return KTV.http.get(base() + '/history');
        },
        usually: function () {
            return KTV.http.get(base() + '/usually');
        },
        setTop: function (songId) {
            return KTV.http.post(base() + '/songs/' + songId + '/top');
        },
        remove: function (songId) {
            return KTV.http.delete(base() + '/songs/' + songId);
        },
    };
})(window.KTV);
