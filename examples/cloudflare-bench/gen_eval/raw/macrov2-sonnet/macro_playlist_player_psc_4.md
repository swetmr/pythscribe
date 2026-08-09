<!-- exp=macrov2-sonnet task=macro_playlist_player cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.05105699999999999 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 101, "title": "Sunset Drive", "artist": "Aurora Wave", "duration": "3:24"},
        {"id": 102, "title": "Slow Tide", "artist": "Marina Low", "duration": "4:01"},
        {"id": 103, "title": "Soft Static", "artist": "Nettle", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 201, "title": "Overdrive", "artist": "Kilo Watt", "duration": "3:12"},
        {"id": 202, "title": "Sprint", "artist": "Redline", "duration": "2:47"},
        {"id": 203, "title": "Pulse Check", "artist": "Vantablack", "duration": "3:33"},
        {"id": 204, "title": "Ironclad", "artist": "Kilo Watt", "duration": "4:10"},
    ]},
    {"id": 3, "name": "Late Night Focus", "tracks": [
        {"id": 301, "title": "Quiet Circuit", "artist": "Halide", "duration": "5:02"},
        {"id": 302, "title": "Amber Room", "artist": "Coral Static", "duration": "4:45"},
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
        set_now_playing(None)
        set_is_playing(False)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            ul(*[
                li(key=p["id"],
                   cn="playlist-item active" if p["id"] == active_id else "playlist-item",
                   oc=lambda: select_playlist(p["id"]),
                   div(cn="playlist-name", p["name"]),
                   div(cn="playlist-count", f'{len(p["tracks"])} tracks'),
                ) for p in PLAYLISTS
            ]),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(*[
                li(key=t["id"],
                   cn="track-item now-playing" if now_playing and t["id"] == now_playing["id"] else "track-item",
                   oc=lambda: select_track(t),
                   span(cn="track-title", t["title"]),
                   span(cn="track-artist", t["artist"]),
                   span(cn="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="bottom-bar",
            div(cn="now-playing-title", now_playing["title"] if now_playing else "No track selected"),
            button(oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
