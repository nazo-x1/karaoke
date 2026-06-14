/**
 * @param obj 页码标签对象
 * @param pageNum 分页总数
 * @param currentpage 当前页
 * @param pageHandler 翻页回调函数名，默认 get_song_list
 */
function PagingManage(obj, pageNum, currentpage, pageHandler) {
    const goPage = pageHandler || 'get_song_list';
    if (obj) {
        let showPageNum = 7;
        let pagehtml = "";

        if (pageNum <= 1) {
            pagehtml = "";
        }

        if (pageNum > 1) {
            if (currentpage > 1) {
                pagehtml += '<li><a href="#" onclick="' + goPage + '(' + (currentpage - 1) + ')">上一页</a></li>';
            }

            if (showPageNum >= pageNum) {
                for (let i = 1; i <= showPageNum; i++) {
                if (i > pageNum) {
                    break;
                }
                if (i === currentpage) {
                    pagehtml += '<li><a class="active" href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                } else {
                    pagehtml += '<li><a href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                }
            }
            } else {
                if (currentpage < 4) {
                    for (let i = 1; i <= 5; i++) {
                        if (i === currentpage) {
                            pagehtml += '<li><a class="active" href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                        } else {
                            pagehtml += '<li><a href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                        }
                    }
                    pagehtml += '<li><a>...</a></li>';
                    pagehtml += '<li><a href="#" onclick="' + goPage + '(' + pageNum + ')">' + pageNum + '</a></li>';
                } else if (currentpage > pageNum-3) {
                    pagehtml += '<li><a href="#" onclick="' + goPage + '(' + 1 + ')">' + 1 + '</a></li>';
                    pagehtml += '<li><a>...</a></li>';
                    for (let i = pageNum-4; i <= pageNum; i++) {
                        if (i === currentpage) {
                            pagehtml += '<li><a class="active" href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                        } else {
                            pagehtml += '<li><a href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                        }
                    }
                } else {
                    pagehtml += '<li><a href="#" onclick="' + goPage + '(' + 1 + ')">' + 1 + '</a></li>';
                    pagehtml += '<li><a>...</a></li>';
                    for (let i = currentpage-1; i <= currentpage+1; i++) {
                        if (i === currentpage) {
                            pagehtml += '<li><a class="active" href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                        } else {
                            pagehtml += '<li><a href="#" onclick="' + goPage + '(' + i + ')">' + i + '</a></li>';
                        }
                    }
                    pagehtml += '<li><a>...</a></li>';
                    pagehtml += '<li><a href="#" onclick="' + goPage + '(' + pageNum + ')">' + pageNum + '</a></li>';
                }
            }
            if (currentpage < pageNum) {
                pagehtml += '<li><a href="#" onclick="' + goPage + '(' + (currentpage + 1) + ')">下一页</a></li>';
            }
        }
        obj.html(pagehtml);
    }
}
