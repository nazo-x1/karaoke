#!/usr/bin/env python
# -*- coding: utf-8 -*-
# @Author: leeyoshinari

import os
import socket
from fastapi import FastAPI, Request
from fastapi.templating import Jinja2Templates
from fastapi.staticfiles import StaticFiles
from tortoise.contrib.fastapi import register_tortoise
import settings
import karaoke.urls as my_urls


prefix = ''
app = FastAPI()
register_tortoise(app=app, config=settings.TORTOISE_ORM)
templates = Jinja2Templates(directory="templates")
app.mount("/static", StaticFiles(directory="static"), name="static")


def get_local_ip():
    try:
        s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
        s.connect(("114.114.114.114", 80))
        host = s.getsockname()[0]
    except Exception:
        host = "0.0.0.0"
    finally:
        s.close()
    return host


@app.get(prefix + "/")
async def index(request: Request):
    return templates.TemplateResponse(
        request=request, name="index.html", context={"request": request, "prefix": prefix}
    )


@app.get(prefix + "/song/edit/{song_id}")
async def song_edit_page(request: Request, song_id: int):
    return templates.TemplateResponse(
        request=request,
        name="song_edit.html",
        context={"request": request, "prefix": prefix, "song_id": song_id},
    )


@app.get(prefix + "/sing")
async def play(request: Request):
    return templates.TemplateResponse(
        request=request,
        name="playing.html",
        context={"request": request, "prefix": prefix},
    )


@app.get(prefix + "/song")
async def deal_song(request: Request):
    return templates.TemplateResponse(
        request=request, name="client.html", context={"request": request, "prefix": prefix}
    )


app.include_router(my_urls.router, prefix=prefix)


if __name__ == "__main__":
    import uvicorn
    local_ip = settings.get_config('host')
    if not local_ip:
        local_ip = get_local_ip()
    uvicorn.run(app="main:app", host=local_ip, port=int(settings.get_config('port')), reload=False)
