const server = KTV.config.server;
let songListTimeout = null;

function originLabel(v) {
    return v === 'upload' ? '上传' : '扫描';
}

function modeLabel(item) {
    if (item.playback_mode === 'enhanced') {
        const src = item.playback_source || '';
        if (src === 'embedded') return '增强(内嵌)';
        return '增强';
    }
    return '仅播放';
}

function statusLabel(item) {
    return item.can_queue ? '<span class="status-ok">可点歌</span>' : '<span class="status-bad">不可用</span>';
}

function toastMsg(msg, fallback) {
    const text = (msg || '').trim();
    return text && text !== 'Success!' ? text : fallback;
}

document.getElementById("file-upload").addEventListener('click', () => {
    let fileUpload_input = document.getElementById("file-input");
    fileUpload_input.click();
    fileUpload_input.onchange = function (event) {
        show_modal_cover();
        let files = event.target.files;
        let total_files = files.length;
        if (total_files < 1) {
            close_modal_cover();
            return;
        }
        let success_num = 0;
        let failure_num = 0;
        let failure_file = [];
        let finished = 0;

        for (let i = 0; i < total_files; i++) {
            let form_data = new FormData();
            form_data.append("file", files[i]);

            let xhr = new XMLHttpRequest();
            xhr.open("POST", KTV.config.apiBase + "/library/upload");
            xhr.onreadystatechange = function() {
                if (xhr.readyState === 4) {
                    finished += 1;
                    if (xhr.status === 200) {
                        let res = JSON.parse(xhr.responseText);
                        if (res['code'] === 0) {
                            success_num += 1;
                        } else {
                            failure_num += 1;
                            failure_file.push(res['data'] || files[i].name);
                        }
                    } else {
                        failure_num += 1;
                        failure_file.push(files[i].name);
                    }
                    if (finished === total_files) {
                        let msg = "";
                        let level = "success";
                        if (success_num > 0) {
                            msg += success_num + "个文件上传成功";
                        }
                        if (failure_num > 0) {
                            if (msg.length > 0) { msg += '，'; }
                            msg += failure_num + "个文件上传失败";
                            level = "error";
                        }
                        $.Toast(msg, level);
                        if (failure_num > 0) {
                            $.Toast(failure_file.join('，'), 'error');
                        }
                        fileUpload_input.value = '';
                        close_modal_cover();
                        get_song_list();
                    }
                }
            };
            xhr.send(form_data);
        }
    };
});

document.getElementById("scan-import").addEventListener('click', (e) => {
    e.preventDefault();
    document.getElementById("scan-modal").classList.add('open');
});

document.getElementById("scan-close-btn").addEventListener('click', () => {
    document.getElementById("scan-modal").classList.remove('open');
});

document.getElementById("scan-preview-btn").addEventListener('click', () => {
    runScanPreview();
});

document.getElementById("scan-run-btn").addEventListener('click', () => {
    runScanExecute();
});

function runScanPreview() {
    const root = document.getElementById("scan-root").value.trim();
    const policy = document.getElementById("scan-policy").value;
    const validate = document.getElementById("scan-validate").checked;
    if (!root) {
        $.Toast("请输入扫描路径", "error");
        return;
    }
    show_modal_cover();
    KTV.library.scanPreview(root, policy, validate).then(function (data) {
        close_modal_cover();
        const d = data.data;
        document.getElementById("scan-preview").innerText =
            `预览：新增 ${d.added}，跳过 ${d.skipped}，重命名 ${d.renamed}，无效 ${d.invalid}`;
    }).catch(function (err) {
        close_modal_cover();
        $.Toast(toastMsg(err.msg, "预览失败"), 'error');
    });
}

function pollScanValidateStatus() {
    const preview = document.getElementById("scan-preview");
    const tick = function () {
        KTV.library.scanStatus().then(function (res) {
            const s = res.data || {};
            if (s.running) {
                preview.innerText =
                    `后台校验中：${s.done}/${s.total}，无效 ${s.invalid}`;
                setTimeout(tick, 800);
            } else {
                preview.innerText =
                    `校验完成：共 ${s.total}，无效 ${s.invalid}`;
                $.Toast(`校验完成：无效 ${s.invalid}`, "success");
                get_song_list();
            }
        }).catch(function () {
            setTimeout(tick, 1500);
        });
    };
    tick();
}

function runScanExecute() {
    const root = document.getElementById("scan-root").value.trim();
    const policy = document.getElementById("scan-policy").value;
    const validate = document.getElementById("scan-validate").checked;
    if (!root) {
        $.Toast("请输入扫描路径", "error");
        return;
    }
    show_modal_cover();
    KTV.library.scan({ root, duplicate_policy: policy, validate }).then(function (data) {
        close_modal_cover();
        const d = data.data;
        const pending = d.pending_validate || 0;
        if (validate && pending > 0) {
            $.Toast(
                `落库完成：新增 ${d.added}，跳过 ${d.skipped}，重命名 ${d.renamed}；正在后台校验 ${pending} 个文件`,
                "success"
            );
            document.getElementById("scan-preview").innerText =
                `后台校验中：0/${pending}，无效 0`;
            pollScanValidateStatus();
        } else {
            $.Toast(
                `扫描完成：新增 ${d.added}，跳过 ${d.skipped}，重命名 ${d.renamed}`,
                "success"
            );
            document.getElementById("scan-modal").classList.remove('open');
        }
        get_song_list();
    }).catch(function (err) {
        close_modal_cover();
        $.Toast(toastMsg(err.msg, "扫描失败"), 'error');
    });
}

document.getElementById("file-search").addEventListener('input', () => {
    clearTimeout(songListTimeout);
    songListTimeout = setTimeout(() => { get_song_list(); }, 500);
});

function get_song_list(page = 1) {
    let q = document.getElementById("file-search").value;
    KTV.library.list(page, q).then(function (data) {
        let s = '';
        const prepHint = document.getElementById("prepare-hint");
        if (prepHint) {
            const n = data.preparing_count || 0;
            prepHint.textContent = n > 0 ? `${n} 首正在后台准备` : '';
            prepHint.style.display = n > 0 ? '' : 'none';
        }
        if (data.total === 0) {
            $.Toast("没有歌曲", "error");
            document.getElementsByTagName("tbody")[0].innerHTML = '';
            PagingManage($('#paging'), 0, 1);
            return;
        }
        data.data.forEach(item => {
            s = s + `<tr>
                <td>${item.display_name}</td>
                <td>${originLabel(item.source_origin)}</td>
                <td>${modeLabel(item)}</td>
                <td>${statusLabel(item)}</td>
                <td>${item.create_time}</td>
                <td><a href="${server}/song/edit/${item.id}">编辑</a><a onclick="delete_song(${item.id}, '${item.source_origin}')">删除</a></td>
            </tr>`;
        });
        PagingManage($('#paging'), data.totalPage, data.page);
        document.getElementsByTagName("table")[0].style.display = "";
        document.getElementById("create-time").style.display = "";
        document.getElementsByTagName("tbody")[0].innerHTML = s;
    }).catch(function (err) {
        $.Toast(toastMsg(err.msg, '加载失败'), 'error');
    });
}

function delete_song(file_id, source_origin) {
    let delete_disk = false;
    if (source_origin === 'upload') {
        delete_disk = confirm("是否同时删除 __keep__ 中的上传文件？\n取消则仅删除数据库记录。");
    }
    KTV.library.remove(file_id, delete_disk).then(function (data) {
        $.Toast(toastMsg(data.msg, "删除成功"), "success");
        get_song_list();
    }).catch(function (err) {
        $.Toast(toastMsg(err.msg, "删除失败"), "error");
    });
}

function show_modal_cover() {
    $('.modal_cover')[0].style.display = 'flex';
    $('.modal_cover>.modal_gif')[0].style.display = 'flex';
}

function close_modal_cover() {
    $('.modal_cover')[0].style.display = 'none';
    $('.modal_cover>.modal_gif')[0].style.display = 'none';
}

window.onload = function() {
    get_song_list();
};
