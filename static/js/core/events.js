window.KTV = window.KTV || {};

(function (KTV) {
    var source = null;
    var handlers = {};

    KTV.events = {
        on: function (code, fn) {
            if (!handlers[code]) handlers[code] = [];
            handlers[code].push(fn);
        },
        connect: function () {
            if (source) return source;
            source = new EventSource(KTV.config.apiBase + '/events');
            source.onmessage = function (event) {
                var message = JSON.parse(event.data);
                var list = handlers[message.code] || [];
                list.forEach(function (fn) { fn(message); });
            };
            source.onerror = function (event) {
                console.error('EventSource failed:', event);
            };
            return source;
        },
        send: function (code, data) {
            return KTV.http.post(KTV.config.apiBase + '/events/command', { code: code, data: data });
        },
    };
})(window.KTV);
