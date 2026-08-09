# Spotify clone - canonical PythScribe track, dual-track with SpotifyApp.tsx
# (the React oracle). All three tracks (.tsx/.ps/.psc) must render identical
# DOM for the same props (see SpotifyApp.test.tsx).
#
# Feature surface (the PythScribe stress test):
#  - sidebar playlist nav + "Up next" queue panel
#  - track table (number / title / artist / duration, hover play, row menu
#    with add-to-queue, active-row highlight)
#  - persistent player bar driving a REAL <audio> element (/media/sample.wav):
#    play/pause, next/prev over the queue, seek bound to timeupdate, volume,
#    elapsed/total time, auto-advance on `ended`
#  - deterministic seeded shuffle (Park-Miller LCG, fixed SHUFFLE_SEED)
#  - Space toggles playback unless focus is in a form field
#  - playback state machine in use_reducer
#
# seeded_shuffle + SHUFFLE_SEED are re-implemented here verbatim from
# fixtures.ts (PythScribe cannot yet import from .ts modules); the parity
# suite asserts the queue orders match the fixtures.ts copy, so any
# cross-language drift fails tests.
#
# NOTE: `#` comment block, not a triple-quoted module docstring - see
# CONTRIBUTING.md "Known friction" (Turbopack UTF-8 char-boundary panic).
"use client"

import "./SpotifyApp.css"

from pyths.react import component, use_state, use_effect, use_reducer, use_ref

SHUFFLE_SEED = 1337


def seeded_shuffle(items, seed):
    arr = items[:]
    s = seed
    i = len(arr) - 1
    while i > 0:
        s = (s * 48271) % 2147483647
        j = s % (i + 1)
        tmp = arr[i]
        arr[i] = arr[j]
        arr[j] = tmp
        i -= 1
    return arr


def reducer(state, action):
    if action["type"] == "SELECT_PLAYLIST":
        return {**state, "playlistId": action["id"]}
    if action["type"] == "PLAY_TRACK":
        tracks = action["tracks"]
        pos = -1
        for i, t in enumerate(tracks):
            if t["id"] == action["trackId"]:
                pos = i
        if pos < 0:
            return state
        if state["shuffle"]:
            rest = [t for t in tracks if t["id"] != action["trackId"]]
            queue = [tracks[pos], *seeded_shuffle(rest, SHUFFLE_SEED)]
            return {**state, "queue": queue, "index": 0, "status": "playing"}
        return {**state, "queue": tracks[:], "index": pos, "status": "playing"}
    if action["type"] == "TOGGLE_PLAY":
        if state["status"] == "playing":
            return {**state, "status": "paused"}
        if len(state["queue"]) == 0:
            return state
        if state["index"] < 0:
            return {**state, "index": 0, "status": "playing"}
        return {**state, "status": "playing"}
    if action["type"] == "NEXT":
        if state["index"] + 1 < len(state["queue"]):
            return {**state, "index": state["index"] + 1, "status": "playing"}
        return {**state, "status": "paused"}
    if action["type"] == "PREV":
        if state["index"] > 0:
            return {**state, "index": state["index"] - 1, "status": "playing"}
        return state
    if action["type"] == "ENDED":
        if state["index"] + 1 < len(state["queue"]):
            return {**state, "index": state["index"] + 1, "status": "playing"}
        return {**state, "status": "idle"}
    if action["type"] == "ADD_TO_QUEUE":
        return {**state, "queue": state["queue"] + [action["track"]]}
    if action["type"] == "TOGGLE_SHUFFLE":
        on = not state["shuffle"]
        if not on or len(state["queue"]) == 0:
            return {**state, "shuffle": on}
        head = state["queue"][:state["index"] + 1]
        rest = seeded_shuffle(state["queue"][state["index"] + 1:], SHUFFLE_SEED)
        return {**state, "shuffle": on, "queue": head + rest}
    if action["type"] == "SET_VOLUME":
        return {**state, "volume": action["value"]}
    return state


def fmt_time(sec):
    total = int(sec)
    m = int(total / 60)
    s = total % 60
    return str(m) + ":" + ("0" + str(s) if s < 10 else str(s))


@c
def TrackRow(track, num, active, menu_open, on_play, on_toggle_menu, on_add):
    return tr(
        cn="sp-track-row active" if active else "sp-track-row",
        data_testid="track-row-" + track["id"],
        data_active="true" if active else "false",
        td(
            cn="col-num",
            span(cn="sp-track-num", num),
            button(
                cn="sp-row-play",
                data_testid="track-play-" + track["id"],
                aria_label="Play " + track["title"],
                oc=lambda: on_play(track),
                "▶",
            ),
        ),
        td(cn="sp-track-title", track["title"]),
        td(cn="sp-track-artist", track["artist"]),
        td(cn="col-dur", fmt_time(track["duration"])),
        td(
            cn="col-menu",
            button(
                cn="sp-row-menu-btn",
                data_testid="track-menu-" + track["id"],
                aria_label="More options for " + track["title"],
                oc=lambda: on_toggle_menu(track),
                "⋯",
            ),
            menu_open and div(
                cn="sp-row-menu",
                data_testid="row-menu-" + track["id"],
                button(
                    data_testid="queue-add-" + track["id"],
                    oc=lambda: on_add(track),
                    "Add to queue",
                ),
            ),
        ),
    )


@c
def SpotifyApp(playlists):
    state, dispatch = use_reducer(reducer, {
        "playlistId": playlists[0]["id"],
        "queue": [],
        "index": -1,
        "status": "idle",
        "shuffle": False,
        "volume": 0.8,
    })
    audio_ref = ur(None)
    current_time, set_current_time = us(0)
    duration, set_duration = us(0)
    menu_id, set_menu_id = us(None)

    matches = [pl for pl in playlists if pl["id"] == state["playlistId"]]
    playlist = matches[0] if len(matches) > 0 else playlists[0]
    current = state["queue"][state["index"]] if state["index"] >= 0 else None
    current_id = current["id"] if current else None
    up_next = state["queue"][state["index"] + 1:]

    # Restart from 0 whenever the active track changes (every fixture track
    # shares the same offline asset, so the src attribute never changes).
    # NOTE: effect functions must not `return None` explicitly - React treats
    # any non-undefined return value as a cleanup function ("destroy is not a
    # function" on unmount). Bare `return` compiles to `return;` (undefined).
    def _reset_on_track_change():
        audio = audio_ref.current
        if not audio:
            return
        try:
            audio.currentTime = 0
        except Exception:
            pass
        set_current_time(0)

    ue(_reset_on_track_change, [current_id])

    # Drive the real <audio> element from the state machine.
    def _sync_playback():
        audio = audio_ref.current
        if not audio:
            return
        if state["status"] == "playing" and current_id:
            p = None
            try:
                p = audio.play()
            except Exception:
                p = None
            if p is not None and p.catch:
                p.catch(lambda _e: None)
        else:
            try:
                audio.pause()
            except Exception:
                pass

    ue(_sync_playback, [state["status"], current_id])

    def _sync_volume():
        audio = audio_ref.current
        if audio:
            audio.volume = state["volume"]

    ue(_sync_volume, [state["volume"]])

    # Space toggles playback - unless focus is in a form field.
    def _space_toggle():
        def on_key(e):
            if e.code != "Space":
                return
            el = e.target
            tag = el.tagName if el and el.tagName else ""
            if tag == "INPUT" or tag == "TEXTAREA" or tag == "SELECT":
                return
            if el and el.isContentEditable:
                return
            e.preventDefault()
            dispatch({"type": "TOGGLE_PLAY"})

        window.addEventListener("keydown", on_key)
        return lambda: window.removeEventListener("keydown", on_key)

    ue(_space_toggle, [])

    def play_track(track):
        dispatch({"type": "PLAY_TRACK", "tracks": playlist["tracks"], "trackId": track["id"]})

    def toggle_menu(track):
        set_menu_id(None if menu_id == track["id"] else track["id"])

    def add_to_queue(track):
        dispatch({"type": "ADD_TO_QUEUE", "track": track})
        set_menu_id(None)

    def on_seek(e):
        v = float(e.target.value)
        audio = audio_ref.current
        if audio:
            try:
                audio.currentTime = v
            except Exception:
                pass
        set_current_time(v)

    def on_time_update(_e):
        audio = audio_ref.current
        if audio:
            set_current_time(audio.currentTime)

    def on_loaded_metadata(_e):
        audio = audio_ref.current
        if audio:
            set_duration(audio.duration)

    return div(
        cn="sp-app",
        data_testid="spotify-app",
        div(
            cn="sp-body",
            aside(
                cn="sp-sidebar",
                data_testid="sp-sidebar",
                h2(cn="sp-logo", "Spotify"),
                ul(
                    cn="sp-playlists",
                    data_testid="sp-playlists",
                    [li(
                        key=pl["id"],
                        button(
                            cn="sp-playlist-btn active" if pl["id"] == state["playlistId"] else "sp-playlist-btn",
                            data_testid="playlist-" + pl["id"],
                            data_active="true" if pl["id"] == state["playlistId"] else "false",
                            # NOTE: comprehensions compile to .map() callbacks, so `pl`
                            # is a fresh per-iteration binding - a plain closure is safe
                            # here (no Python late-binding pitfall, and the lambda-IIFE
                            # alternative miscompiles; see bug log in the clone report).
                            on_click=lambda: dispatch({"type": "SELECT_PLAYLIST", "id": pl["id"]}),
                            pl["name"],
                        ),
                    ) for pl in playlists],
                ),
                div(
                    cn="sp-queue",
                    data_testid="sp-queue",
                    h3("Up next"),
                    p(cn="sp-queue-empty", data_testid="queue-empty", "Queue is empty")
                    if len(up_next) == 0
                    else ol(
                        cn="sp-queue-list",
                        data_testid="queue-list",
                        [li(key=i, cn="sp-queue-item", data_testid="queue-item", t["title"]) for i, t in enumerate(up_next)],
                    ),
                ),
            ),
            main(
                cn="sp-main",
                h1(data_testid="sp-playlist-title", playlist["name"]),
                p(cn="sp-playlist-meta", data_testid="sp-playlist-meta", str(len(playlist["tracks"])) + " tracks"),
                table(
                    cn="sp-tracks",
                    data_testid="sp-track-table",
                    thead(
                        tr(
                            th(cn="col-num", "#"),
                            th("Title"),
                            th("Artist"),
                            th(cn="col-dur", "Time"),
                            th(cn="col-menu"),
                        ),
                    ),
                    tbody(
                        [TrackRow(
                            key=t["id"],
                            track=t,
                            num=i + 1,
                            active=(current["id"] == t["id"]) if current else False,
                            menu_open=menu_id == t["id"],
                            on_play=play_track,
                            on_toggle_menu=toggle_menu,
                            on_add=add_to_queue,
                        ) for i, t in enumerate(playlist["tracks"])],
                    ),
                ),
            ),
        ),
        footer(
            cn="sp-player",
            data_testid="sp-player",
            div(
                cn="sp-now",
                span(cn="sp-now-title", data_testid="player-track-title", current["title"] if current else "Nothing playing"),
                span(cn="sp-now-artist", data_testid="player-track-artist", current["artist"] if current else ""),
            ),
            div(
                cn="sp-controls",
                button(
                    cn="sp-ctl sp-shuffle on" if state["shuffle"] else "sp-ctl sp-shuffle",
                    data_testid="player-shuffle",
                    aria_pressed="true" if state["shuffle"] else "false",
                    aria_label="Toggle shuffle",
                    oc=lambda: dispatch({"type": "TOGGLE_SHUFFLE"}),
                    "🔀",
                ),
                button(
                    cn="sp-ctl",
                    data_testid="player-prev",
                    aria_label="Previous track",
                    oc=lambda: dispatch({"type": "PREV"}),
                    "⏮",
                ),
                button(
                    cn="sp-ctl sp-play-btn",
                    data_testid="player-play",
                    aria_label="Pause" if state["status"] == "playing" else "Play",
                    oc=lambda: dispatch({"type": "TOGGLE_PLAY"}),
                    "⏸" if state["status"] == "playing" else "▶",
                ),
                button(
                    cn="sp-ctl",
                    data_testid="player-next",
                    aria_label="Next track",
                    oc=lambda: dispatch({"type": "NEXT"}),
                    "⏭",
                ),
            ),
            div(
                cn="sp-timeline",
                span(cn="sp-time", data_testid="player-elapsed", fmt_time(current_time)),
                input(
                    type="range",
                    cn="sp-seek",
                    data_testid="player-seek",
                    aria_label="Seek",
                    min=0,
                    max=duration if duration > 0 else 0,
                    step=0.01,
                    value=current_time if current_time < duration else (duration if duration > 0 else 0),
                    oh=on_seek,
                ),
                span(cn="sp-time", data_testid="player-total", fmt_time(duration if duration > 0 else 0)),
            ),
            div(
                cn="sp-volume",
                input(
                    type="range",
                    cn="sp-vol",
                    data_testid="player-volume",
                    aria_label="Volume",
                    min=0,
                    max=1,
                    step=0.01,
                    value=state["volume"],
                    oh=lambda e: dispatch({"type": "SET_VOLUME", "value": float(e.target.value)}),
                ),
            ),
            audio(
                ref=audio_ref,
                data_testid="player-audio",
                src=current["src"] if current else None,
                preload="auto",
                on_time_update=on_time_update,
                on_loaded_metadata=on_loaded_metadata,
                on_ended=lambda _e: dispatch({"type": "ENDED"}),
            ),
        ),
    )
