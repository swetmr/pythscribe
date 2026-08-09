<!-- exp=macrov2-opus task=macro_playlist_player cond=ps sample=3 model=claude-opus-4-8 cost_usd=0.051741999999999996 -->
```python
"use client"
from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drive", "artist": "Nova Lane", "duration": "3:42"},
            {"id": "t2", "title": "Ocean Breath", "artist": "Marisol", "duration": "4:15"},
            {"id": "t3", "title": "Slow Morning", "artist": "Kite", "duration": "2:58"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"id": "t4", "title": "Quiet Circuits", "artist": "Ambient Co", "duration": "5:20"},
            {"id": "t5", "title": "Paper Trails", "artist": "Lo Fern", "duration": "3:11"},
            {"id": "t6", "title": "Grey Rain", "artist": "Halcyon", "duration": "4:47"},
            {"id": "t7", "title": "Still Air", "artist": "Nova Lane", "duration": "3:33"},
        ],
    },
    {
        "id": "hype",
        "name": "Workout Hype",
        "tracks": [
            {"id": "t8", "title": "Redline", "artist": "Pulse", "duration": "2:44"},
            {"id": "t9", "title": "Overdrive", "artist": "Volt", "duration": "3:05"},
            {"id": "t10", "title": "Last Rep", "artist": "Marisol", "duration": "3:50"},
        ],
    },
]


def find_playlist(pid):
    for pl in PLAYLISTS:
        if pl["id"] == pid:
            return pl
    return PLAYLISTS[0]


def find_track(pid, tid):
    if tid is None:
        return None
    for t in find_playlist(pid)["tracks"]:
        if t["id"] == tid:
            return t
    return None


@component
def PlaylistPlayer():
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active = find_playlist(active_id)
    now_playing = find_track(active_id, now_playing_id)

    def select_playlist(pid):
        set_active_id(pid)
        set_now_playing_id(None)
        set_is_playing(False)

    def play_track(tid):
        set_now_playing_id(tid)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="pp-body",
            aside(class_name="pp-sidebar",
                h2(class_name="pp-sidebar-title", "Your Playlists"),
                ul(class_name="pp-playlist-list",
                    *[li(key=pl["id"],
                        class_name="pp-playlist-item active" if pl["id"] == active_id else "pp-playlist-item",
                        on_click=lambda: select_playlist(pl["id"]),
                        span(class_name="pp-playlist-name", pl["name"]),
                        span(class_name="pp-playlist-count", f"{len(pl['tracks'])} tracks"),
                    ) for pl in PLAYLISTS]),
            ),
            main(class_name="pp-main",
                h2(class_name="pp-main-title", active["name"]),
                ul(class_name="pp-track-list",
                    *[li(key=t["id"],
                        class_name="pp-track active" if t["id"] == now_playing_id else "pp-track",
                        on_click=lambda: play_track(t["id"]),
                        span(class_name="pp-track-title", t["title"]),
                        span(class_name="pp-track-artist", t["artist"]),
                        span(class_name="pp-track-duration", t["duration"]),
                    ) for t in active["tracks"]]),
            ),
        ),
        div(class_name="pp-nowplaying-bar",
            div(class_name="pp-nowplaying-info",
                span(class_name="pp-nowplaying-label", "Now Playing"),
                span(class_name="pp-nowplaying-title",
                     f"{now_playing['title']} — {now_playing['artist']}" if now_playing is not None else "Nothing selected"),
            ),
            button(class_name="pp-play-toggle",
                   on_click=lambda: toggle_play(),
                   disabled=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
