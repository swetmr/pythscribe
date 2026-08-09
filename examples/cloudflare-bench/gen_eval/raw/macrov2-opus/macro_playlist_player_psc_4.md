<!-- exp=macrov2-opus task=macro_playlist_player cond=psc sample=4 model=claude-opus-4-8 cost_usd=0.043303 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "chill", "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Lofi Cat", "duration": "3:42"},
        {"title": "Ocean Eyes", "artist": "Marea", "duration": "4:05"},
        {"title": "Slow Morning", "artist": "Kettle", "duration": "2:58"},
    ]},
    {"id": "focus", "name": "Deep Focus", "tracks": [
        {"title": "Neural Net", "artist": "Circuit", "duration": "5:12"},
        {"title": "Flow State", "artist": "Ambient Co", "duration": "6:30"},
    ]},
    {"id": "hype", "name": "Workout Hype", "tracks": [
        {"title": "Ignite", "artist": "Volt", "duration": "3:20"},
        {"title": "Peak", "artist": "Runner", "duration": "3:55"},
        {"title": "Overdrive", "artist": "Nitro", "duration": "4:10"},
        {"title": "Final Rep", "artist": "Crush", "duration": "2:45"},
    ]},
]


@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = next((p for p in PLAYLISTS if p["id"] == active_id), PLAYLISTS[0])

    def select_playlist(pid):
        set_active_id(pid)
        set_now_playing(None)
        set_is_playing(False)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="pp-body",
            aside(cn="pp-sidebar",
                h2(cn="pp-sidebar-title", "Playlists"),
                ul(cn="pp-playlist-list",
                    *[li(key=p["id"],
                         cn="pp-playlist-item active" if p["id"] == active_id else "pp-playlist-item",
                         oc=lambda: select_playlist(p["id"]),
                         span(cn="pp-playlist-name", p["name"]),
                         span(cn="pp-playlist-count", f"{len(p['tracks'])} tracks"))
                      for p in PLAYLISTS]),
            ),
            section(cn="pp-main",
                h2(cn="pp-main-title", active["name"]),
                ul(cn="pp-track-list",
                    *[li(key=f"{active['id']}-{i}",
                         cn="pp-track active" if now_playing is not None and now_playing["title"] == t["title"] else "pp-track",
                         oc=lambda: play_track(t),
                         span(cn="pp-track-title", t["title"]),
                         span(cn="pp-track-artist", t["artist"]),
                         span(cn="pp-track-duration", t["duration"]))
                      for i, t in enumerate(active["tracks"])]),
            ),
        ),
        div(cn="pp-bottom-bar",
            div(cn="pp-now-playing",
                span(cn="pp-now-label", "Now Playing:"),
                span(cn="pp-now-title",
                     now_playing["title"] if now_playing is not None else "—"),
            ),
            button(cn="pp-play-toggle",
                   dis=now_playing is None,
                   oc=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
