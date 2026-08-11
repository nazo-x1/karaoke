window.KTV = window.KTV || {};

(function (KTV) {
    var base = function () { return KTV.config.apiBase + '/workshop'; };

    KTV.workshop = {
        createSession: function () {
            return KTV.http.post(base() + '/sessions');
        },
        getSession: function (sessionId) {
            return KTV.http.get(base() + '/sessions/' + sessionId);
        },
        destroySession: function (sessionId) {
            return KTV.http.delete(base() + '/sessions/' + sessionId);
        },
        preflight: function (sessionId, formData) {
            return KTV.http.upload(base() + '/sessions/' + sessionId + '/preflight', formData);
        },
        assemble: function (sessionId, formData) {
            return KTV.http.upload(base() + '/sessions/' + sessionId + '/assemble', formData);
        },
        aiSeparate: function (sessionId, formData) {
            return KTV.http.upload(base() + '/sessions/' + sessionId + '/ai-separate', formData);
        },
        commit: function (sessionId, duplicate_policy) {
            return KTV.http.post(
                base() + '/sessions/' + sessionId + '/commit',
                { duplicate_policy: duplicate_policy },
                'application/json'
            );
        },
        features: function () {
            return KTV.http.get(KTV.config.apiBase + '/system/features');
        },
    };
})(window.KTV);
