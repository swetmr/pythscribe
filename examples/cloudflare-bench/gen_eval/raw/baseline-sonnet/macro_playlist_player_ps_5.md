<!-- exp=baseline-sonnet task=macro_playlist_player cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0321957 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {"id": "p1", "name": "Chill Vibes", "tracks": [
        {"id": "t1", "title": "Sunset Drive", "artist": "Nora Lane", "duration": "3:24"},
        {"id": "t2", "title": "Ocean Breeze", "artist": "Kalo Reef", "duration": "4:01"},
        {"id": "t3", "title": "Slow Motion", "artist": "Dust Parade", "duration": "2:58"},
    ]},
    {"id": "p2", "name": "Workout Mix", "tracks": [
        {"id": "t4", "title": "Iron Pulse", "artist": "Volt Runner", "duration": "3:10"},
        {"id": "t5", "title": "Overdrive", "artist": "Static Fox", "duration": "3:47"},
        {"id": "t6", "title": "Push Harder", "artist": "Rell Grey", "duration": "2:39"},
        {"id": "t7", "title": "Redline", "artist": "Volt Runner", "duration": "3:55"},
    ]},
    {"id": "p3", "name": "Late Night Focus", "tracks": [
        {"id": "t8", "title": "Quiet Hours", "artist": "Mira Sol", "duration": "5:12"},
        {"id": "t9", "title": "Paper Lanterns", "artist": "Eno Vale", "duration": "4:33"},
    ]},
    {"id": "p4", "name": "Road Trip Classics", "tracks": [
        {"id": "t10", "title": "Highway Song", "artist": "Cove & Rowe", "duration": "3:20"},
        {"id": "t11", "title": "Open Road", "artist": "Dust Parade", "duration": "3:44"},
        {"id": "t12", "title": "Miles Ahead", "artist": "Nora Lane", "duration": "4:08"},
    ]},
]


def find_playlist(playlist_id):
    for p in PLAYLISTS:
        if p["id"] == playlist_id:
            return p
    return PLAYLISTS[0]


@component
def PlaylistPlayer():
    active_playlist_id, set_active_playlist_id = use_state(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active_playlist = find_playlist(active_playlist_id)

    def select_playlist(playlist_id):
        set_active_playlist_id(playlist_id)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h2(class_name="sidebar-title", "Playlists"),
            ul(class_name="playlist-list",
                *[li(key=p["id"],
                     class_name=f"playlist-item{' active' if p['id'] == active_playlist_id else ''}",
                     on_click=lambda p=p: select_playlist(p["id"]),
                     div(class_name="playlist-name", p["name"]),
                     div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
                  ) for p in PLAYLISTS]
            ),
        ),
        div(class_name="main-panel",
            h2(class_name="main-title", active_playlist["name"]),
            ul(class_name="track-list",
                *[li(key=t["id"],
                     class_name=f"track-item{' now-playing' if now_playing and t['id'] == now_playing['id'] else ''}",
                     on_click=lambda t=t: select_track(t),
                     span(class_name="track-title", t["title"]),
                     span(class_name="track-artist", t["artist"]),
                     span(class_name="track-duration", t["duration"]),
                  ) for t in active_playlist["tracks"]]
            ),
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                now_playing["title"] if now_playing else "No track selected",
            ) if now_playing else div(class_name="now-playing-info", "No track selected"),
            button(class_name="play-pause-btn", on_click=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
