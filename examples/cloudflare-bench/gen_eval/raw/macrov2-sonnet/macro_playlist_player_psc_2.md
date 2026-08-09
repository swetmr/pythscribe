<!-- exp=macrov2-sonnet task=macro_playlist_player cond=psc sample=2 model=claude-sonnet-5 cost_usd=0.032876999999999997 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 101, "title": "Sunset Drive", "artist": "Nova Beach", "duration": "3:24"},
        {"id": 102, "title": "Slow Motion", "artist": "Rue Marlow", "duration": "4:01"},
        {"id": 103, "title": "Ocean Static", "artist": "Kell Loom", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 201, "title": "Iron Pulse", "artist": "Grit Theory", "duration": "3:12"},
        {"id": 202, "title": "Overdrive", "artist": "Max Torque", "duration": "3:47"},
        {"id": 203, "title": "Sprint", "artist": "Vela Kade", "duration": "2:39"},
        {"id": 204, "title": "Redline", "artist": "Grit Theory", "duration": "3:55"},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 301, "title": "Quiet Room", "artist": "Ambient Set", "duration": "5:10"},
        {"id": 302, "title": "Paper Thoughts", "artist": "Lena Cole", "duration": "4:22"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = [p for p in PLAYLISTS if p["id"] == active_id][0]

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    def mk_select(pid):
        return lambda: select_playlist(pid)

    def mk_pick(t):
        return lambda: select_track(t)

    return div(cn="player-app",
        div(cn="player-body",
            aside(cn="sidebar",
                h2("Playlists"),
                ul(*[
                    li(key=p["id"],
                       cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                       oc=mk_select(p["id"]),
                       span(cn="playlist-name", p["name"]),
                       span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                    )
                    for p in PLAYLISTS
                ]),
            ),
            main(cn="track-panel",
                h2(active_playlist["name"]),
                ul(cn="track-list",
                    *[
                        li(key=t["id"],
                           cn="track-item now-playing" if now_playing and now_playing["id"] == t["id"] else "track-item",
                           oc=mk_pick(t),
                           span(cn="track-title", t["title"]),
                           span(cn="track-artist", t["artist"]),
                           span(cn="track-duration", t["duration"]),
                        )
                        for t in active_playlist["tracks"]
                    ]
                ),
            ),
        ),
        div(cn="now-playing-bar",
            span(cn="now-playing-title", now_playing["title"] if now_playing else "Nothing playing"),
            button(cn="play-toggle", oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
