window.KTV = window.KTV || {};

(function (KTV) {
    const server = localStorage.getItem('server') || '';
    KTV.config = {
        server: server,
        apiBase: server + '/api/v1',
    };
})(window.KTV);
