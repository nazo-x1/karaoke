window.KTV = window.KTV || {};

(function (KTV) {
    var base = function () { return KTV.config.apiBase + '/library'; };

    KTV.library = {
        upload: function (formData) {
            return KTV.http.upload(base() + '/upload', formData);
        },
        scanPreview: function (root, duplicate_policy, validate) {
            return KTV.http.get(base() + '/scan/preview', {
                root: root,
                duplicate_policy: duplicate_policy,
                validate: validate,
            });
        },
        scan: function (body) {
            return KTV.http.post(base() + '/scan', JSON.stringify(body), 'application/json');
        },
        list: function (page, q) {
            return KTV.http.get(base() + '/songs', { page: page, q: q || '' });
        },
        remove: function (songId, deleteDisk) {
            return KTV.http.delete(base() + '/songs/' + songId, { delete_disk: deleteDisk });
        },
    };
})(window.KTV);
