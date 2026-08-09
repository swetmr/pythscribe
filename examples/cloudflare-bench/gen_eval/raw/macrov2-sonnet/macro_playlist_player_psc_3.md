<!-- exp=macrov2-sonnet task=macro_playlist_player cond=psc sample=3 model=claude-sonnet-5 cost_usd=0.056442 -->
```python
"use client"
R*

playlists = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 101, "title": "Sunset Drive", "artist": "Nova Wave", "duration": "3:24"},
        {"id": 102, "title": "Ocean Breeze", "artist": "Lena Cole", "duration": "4:01"},
        {"id": 103, "title": "Late Night", "artist": "Nova Wave", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 201, "title": "Power Up", "artist": "Iron Beats", "duration": "3:12"},
        {"id": 202, "title": "Run Faster", "artist": "Iron Beats", "duration": "3:45"},
        {"id": 203, "title": "Max Effort", "artist": "DJ Torque", "duration": "4:10"},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 301, "title": "Deep Work", "artist": "Quiet Mind", "duration": "5:20"},
        {"id": 302, "title": "Clarity", "artist": "Quiet Mind", "duration": "4:48"},
        {"id": 303, "title": "Still Point", "artist": "Aria Lund", "duration": "3:33"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(playlists[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = next((p for p in playlists if p["id"] == active_id), playlists[0])

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="body",
            div(cn="sidebar",
                h2("Playlists"),
                ul(*[
                    li(key=p["id"],
                       cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                       oc=lambda: set_active_id(p["id"]),
                       span(cn="playlist-name", p["name"]),
                       span(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in playlists
                ]),
            ),
            div(cn="main-panel",
                h2(active_playlist["name"]),
                ul(cn="track-list", *[
                    li(key=t["id"],
                       cn="track-row now-playing" if now_playing and t["id"] == now_playing["id"] else "track-row",
                       oc=lambda: (set_now_playing(t), set_is_playing(True)),
                       span(cn="track-title", t["title"]),
                       span(cn="track-artist", t["artist"]),
                       span(cn="track-duration", t["duration"]),
                    ) for t in active_playlist["tracks"]
                ]),
            ),
        ),
        div(cn="now-playing-bar",
            span(cn="now-playing-title", now_playing["title"] if now_playing else "Nothing playing"),
            button(oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
