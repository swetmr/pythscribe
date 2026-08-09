# YouTube clone - canonical PythScribe track, dual-track with YouTubeApp.tsx
# (the React oracle; see the *.test.tsx parity suites in this directory).
#
# SINGLE-MODULE DESIGN (deliberate): all five components live in one
# tri-track module because `pyths compile` emits extensionless relative
# imports and scripts/precompile-client.mjs does not rewrite them - a
# multi-file island's `import './VideoCard'` inside YouTubeApp.client.js
# would resolve to the `.tsx` ORACLE in the Next client graph (bundler
# extension order puts .tsx first), silently un-dogfooding the production
# track. One module per track sidesteps the whole class of problem.
#
# Cross-track prop names are snake_case (`on_open`, `on_back`, ...) so the
# SAME props mount every track in the parity tests.
#
# NOTE: `#` comment block, not a triple-quoted module docstring - avoids the
# Next.js 16 Turbopack UTF-8 char-boundary panic on non-ASCII docstrings
# (see CONTRIBUTING.md "Known friction").
"use client"

import "./YouTubeApp.css"

from pyths.react import component, use_state, use_effect, use_ref, use_memo


def fmt_time(t):
    m = Math.floor(t / 60)
    s = Math.floor(t % 60)
    return f"{m}:0{s}" if s < 10 else f"{m}:{s}"


@c
def SearchHeader(query, on_change):
    return header(
        cn="yt-header",
        data_testid="yt-header",
        div(
            cn="yt-logo",
            span(cn="yt-logo-mark", "▶"),
            span(cn="yt-logo-text", "MyTube"),
        ),
        form(
            cn="yt-search-form",
            os=lambda e: e.preventDefault(),
            input(
                cn="yt-search",
                data_testid="yt-search",
                type="search",
                ph="Search",
                value=query,
                oh=lambda e: on_change(e.target.value),
            ),
            button(cn="yt-search-btn", type="submit", "Search"),
        ),
    )


@c
def VideoCard(item, on_open):
    hovering, set_hovering = us(False)
    return article(
        cn="yt-card",
        data_testid="yt-card",
        oc=lambda: on_open(item),
        on_mouse_enter=lambda: set_hovering(True),
        on_mouse_leave=lambda: set_hovering(False),
        div(
            cn="yt-thumb",
            st={"backgroundImage": f"url({item['thumb']})", "backgroundSize": "cover", "backgroundColor": item["color"]},
            video(
                cn="yt-preview",
                data_testid="yt-preview",
                src="/media/sample.webm",
                auto_play=True,
                muted=True,
                loop=True,
                plays_inline=True,
            ) if hovering else span(cn="yt-thumb-icon", "▶"),
            span(cn="yt-duration", item["duration"]),
        ),
        div(
            cn="yt-meta",
            div(cn="yt-avatar", st={"background": item["color"]}, item["channel"][0]),
            div(
                cn="yt-info",
                h3(cn="yt-card-title", data_testid="yt-card-title", item["title"]),
                p(cn="yt-channel", item["channel"]),
                p(cn="yt-stats", f"{item['views']} views • {item['age']}"),
            ),
        ),
    )


@c
def VideoFeed(videos, on_open):
    visible_count, set_visible_count = us(12)
    sentinel_ref = ur(None)

    def reset():
        set_visible_count(12)

    ue(reset, [videos])

    def observe():
        node = sentinel_ref.current
        if not node:
            return
        def on_intersect(entries):
            if entries[0].isIntersecting:
                set_visible_count(lambda n: Math.min(n + 12, videos.length))
        obs = IntersectionObserver(on_intersect, {"rootMargin": "200px"})
        obs.observe(node)
        return lambda: obs.disconnect()

    ue(observe, [videos])

    shown = videos.slice(0, visible_count)
    return section(
        cn="yt-feed",
        data_testid="yt-feed",
        videos.length == 0 and p(cn="yt-empty", data_testid="yt-empty", "No videos match your search."),
        div(
            cn="yt-grid",
            data_testid="yt-grid",
            *[VideoCard(key=v["id"], item=v, on_open=on_open) for v in shown],
        ),
        visible_count < videos.length and div(cn="yt-sentinel", data_testid="yt-sentinel", ref=sentinel_ref, "Loading more…"),
    )


@c
def WatchView(item, related, subscribed, on_toggle_subscribe, on_open, on_back):
    playing, set_playing = us(False)
    current_time, set_current_time = us(0)
    duration, set_duration = us(0)
    video_ref = ur(None)

    def toggle_play():
        el = video_ref.current
        if not el:
            return
        if playing:
            el.pause()
            set_playing(False)
        else:
            pr = el.play()
            if pr and pr.catch:
                pr.catch(lambda err: None)
            set_playing(True)

    def seek_by(delta):
        el = video_ref.current
        if not el:
            return
        d = el.duration or 0
        t = Math.max(0, Math.min(d, el.currentTime + delta))
        el.currentTime = t
        set_current_time(t)

    def handle_seek(e):
        t = parseFloat(e.target.value)
        el = video_ref.current
        if el:
            el.currentTime = t
        set_current_time(t)

    def handle_key(e):
        tag = e.target and e.target.tagName
        if tag == "INPUT" or tag == "TEXTAREA" or tag == "BUTTON":
            return
        if e.key == " " or e.code == "Space":
            e.preventDefault()
            toggle_play()
        elif e.key == "ArrowRight":
            e.preventDefault()
            seek_by(5)
        elif e.key == "ArrowLeft":
            e.preventDefault()
            seek_by(-5)

    def bind_keys():
        window.addEventListener("keydown", handle_key)
        return lambda: window.removeEventListener("keydown", handle_key)

    ue(bind_keys, [playing])

    return section(
        cn="yt-watch",
        data_testid="yt-watch",
        button(cn="yt-back", data_testid="yt-back", oc=lambda: on_back(), "← Back to feed"),
        div(
            cn="yt-watch-body",
            div(
                cn="yt-primary",
                video(
                    cn="yt-video",
                    data_testid="yt-video",
                    ref=video_ref,
                    src="/media/sample.webm",
                    auto_play=True,
                    muted=True,
                    loop=True,
                    plays_inline=True,
                    on_time_update=lambda e: set_current_time(e.target.currentTime),
                    on_loaded_metadata=lambda e: set_duration(e.target.duration or 0),
                    on_play=lambda: set_playing(True),
                    on_pause=lambda: set_playing(False),
                ),
                div(
                    cn="yt-controls",
                    button(cn="yt-play", data_testid="yt-play", oc=lambda: toggle_play(), "Pause" if playing else "Play"),
                    input(
                        cn="yt-seek",
                        data_testid="yt-seek",
                        type="range",
                        min=0,
                        max=duration or 0,
                        step=0.1,
                        value=current_time,
                        oh=handle_seek,
                    ),
                    span(cn="yt-time", data_testid="yt-time", f"{fmt_time(current_time)} / {fmt_time(duration)}"),
                ),
                h1(cn="yt-watch-title", data_testid="yt-watch-title", item["title"]),
                div(
                    cn="yt-channel-row",
                    div(cn="yt-avatar", st={"background": item["color"]}, item["channel"][0]),
                    div(
                        cn="yt-channel-info",
                        p(cn="yt-channel", item["channel"]),
                        p(cn="yt-stats", f"{item['views']} views • {item['age']}"),
                    ),
                    button(
                        cn="yt-subscribe" + (" subscribed" if subscribed else ""),
                        data_testid="yt-subscribe",
                        oc=lambda: on_toggle_subscribe(item["channel"]),
                        "Subscribed" if subscribed else "Subscribe",
                    ),
                ),
            ),
            aside(
                cn="yt-related",
                data_testid="yt-related",
                h2(cn="yt-related-heading", "Up next"),
                *[
                    div(
                        key=v["id"],
                        cn="yt-related-item",
                        data_testid="yt-related-item",
                        oc=lambda: on_open(v),
                        div(cn="yt-related-thumb", st={"backgroundImage": f"url({v['thumb']})", "backgroundSize": "cover", "backgroundColor": v["color"]}, span(cn="yt-duration", v["duration"])),
                        div(
                            cn="yt-related-meta",
                            p(cn="yt-related-title", v["title"]),
                            p(cn="yt-related-channel", v["channel"]),
                        ),
                    )
                    for v in related
                ],
            ),
        ),
    )


@c
def YouTubeApp(videos):
    query, set_query = us("")
    current, set_current = us(None)
    subscribed_channels, set_subscribed_channels = us([])

    filtered = um(
        lambda: videos.filter(
            lambda v: v["title"].toLowerCase().includes(query.toLowerCase())
            or v["channel"].toLowerCase().includes(query.toLowerCase())
        ),
        [videos, query],
    )

    def handle_query(q):
        set_query(q)
        set_current(None)

    def open_video(v):
        set_current(v)

    def toggle_subscribe(channel):
        if subscribed_channels.includes(channel):
            set_subscribed_channels(subscribed_channels.filter(lambda c: c != channel))
        else:
            set_subscribed_channels([*subscribed_channels, channel])

    related = videos.filter(lambda v: v["id"] != current["id"]).slice(0, 10) if current else []

    return div(
        cn="yt-app",
        data_testid="yt-app",
        SearchHeader(query=query, on_change=handle_query),
        WatchView(
            key=current["id"],
            item=current,
            related=related,
            subscribed=subscribed_channels.includes(current["channel"]),
            on_toggle_subscribe=toggle_subscribe,
            on_open=open_video,
            on_back=lambda: set_current(None),
        ) if current else VideoFeed(videos=filtered, on_open=open_video),
    )


__default__ = YouTubeApp
