<!-- exp=macrov2-sonnet task=macro_playlist_player cond=psc sample=5 model=claude-sonnet-5 cost_usd=0.053187 -->
```python
"use client"
R*

playlists = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 1, "title": "Sunset Drive", "artist": "Nova Sky", "duration": "3:24"},
        {"id": 2, "title": "Ocean Breeze", "artist": "Nova Sky", "duration": "4:01"},
        {"id": 3, "title": "Golden Hour", "artist": "Mellow Tide", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 4, "title": "Push Harder", "artist": "Iron Pulse", "duration": "3:12"},
        {"id": 5, "title": "Sprint", "artist": "Iron Pulse", "duration": "2:45"},
        {"id": 6, "title": "Full Send", "artist": "Kilo Beat", "duration": "3:50"},
        {"id": 7, "title": "Last Rep", "artist": "Kilo Beat", "duration": "3:05"},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 8, "title": "Deep Work", "artist": "Quiet Signal", "duration": "5:10"},
        {"id": 9, "title": "Slow Burn", "artist": "Quiet Signal", "duration": "4:33"},
    ]},
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(playlists[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active_playlist = next((p for p in playlists if p["id"] == active_id), playlists[0])

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(t):
        set_now_playing(t)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="sidebar",
            h2("Playlists"),
            ul(*[
                li(key=p["id"],
                   cn=f"playlist-item{' active' if p['id'] == active_id else ''}",
                   oc=lambda: select_playlist(p["id"]),
                   div(cn="playlist-name", p["name"]),
                   div(cn="playlist-count", f"{len(p['tracks'])} tracks"),
                ) for p in playlists
            ]),
        ),
        div(cn="main-panel",
            h2(active_playlist["name"]),
            ul(cn="track-list", *[
                li(key=t["id"],
                   cn=f"track-item{' now-playing' if now_playing and now_playing['id'] == t['id'] else ''}",
                   oc=lambda: select_track(t),
                   span(cn="track-title", t["title"]),
                   span(cn="track-artist", t["artist"]),
                   span(cn="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]
            ]),
        ),
        div(cn="bottom-bar",
            span(cn="now-playing-title",
                 now_playing["title"] if now_playing else "No track selected"),
            button(oc=lambda: toggle_play(), dis=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
