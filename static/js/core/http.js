window.KTV = window.KTV || {};

(function (KTV) {
    function request(method, url, options) {
        options = options || {};
        return new Promise(function (resolve, reject) {
            $.ajax({
                type: method,
                url: url,
                data: options.data,
                contentType: options.contentType,
                processData: options.processData !== false,
                success: function (res) {
                    if (res && res.code === 0) resolve(res);
                    else reject(res || { msg: '请求失败' });
                },
                error: function (xhr) {
                    reject({ msg: xhr.statusText || '网络错误', xhr: xhr });
                }
            });
        });
    }

    KTV.http = {
        get: function (path, params) {
            let url = path;
            if (params) {
                const qs = Object.keys(params)
                    .filter(function (k) { return params[k] !== undefined && params[k] !== null; })
                    .map(function (k) { return encodeURIComponent(k) + '=' + encodeURIComponent(params[k]); })
                    .join('&');
                if (qs) url += (url.indexOf('?') >= 0 ? '&' : '?') + qs;
            }
            return request('GET', url);
        },
        post: function (path, body, contentType) {
            if (body === undefined) {
                return request('POST', path, { data: null, contentType: contentType });
            }
            return request('POST', path, {
                data: body,
                contentType: contentType || (body && typeof body === 'object' && !(body instanceof FormData)
                    ? 'application/json' : undefined),
                processData: !(body instanceof FormData) && typeof body === 'object',
            });
        },
        patch: function (path, body) {
            return request('PATCH', path, {
                data: JSON.stringify(body),
                contentType: 'application/json',
            });
        },
        delete: function (path, params) {
            let url = path;
            if (params) {
                const qs = Object.keys(params)
                    .filter(function (k) { return params[k] !== undefined && params[k] !== null; })
                    .map(function (k) { return encodeURIComponent(k) + '=' + encodeURIComponent(params[k]); })
                    .join('&');
                if (qs) url += (url.indexOf('?') >= 0 ? '&' : '?') + qs;
            }
            return new Promise(function (resolve, reject) {
                $.ajax({
                    type: 'DELETE',
                    url: url,
                    success: function (res) {
                        if (res && res.code === 0) resolve(res);
                        else reject(res || { msg: '请求失败' });
                    },
                    error: function () { reject({ msg: '网络错误' }); },
                });
            });
        },
        upload: function (path, formData) {
            return request('POST', path, {
                data: formData,
                contentType: false,
                processData: false,
            });
        },
    };
})(window.KTV);
