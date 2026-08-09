<!-- exp=macrov2-sonnet task=macro_playlist_player cond=ps sample=5 model=claude-sonnet-5 cost_usd=0.0311754 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "p1",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drive", "artist": "Nova Wave", "duration": "3:24"},
            {"id": "t2", "title": "Ocean Breeze", "artist": "Lena Marsh", "duration": "4:01"},
            {"id": "t3", "title": "Slow Motion", "artist": "Kite Club", "duration": "2:58"},
        ],
    },
    {
        "id": "p2",
        "name": "Workout Mix",
        "tracks": [
            {"id": "t4", "title": "Push Harder", "artist": "Iron Pulse", "duration": "3:12"},
            {"id": "t5", "title": "Sprint", "artist": "Dax Reyes", "duration": "2:45"},
            {"id": "t6", "title": "Overdrive", "artist": "Vector 9", "duration": "3:37"},
            {"id": "t7", "title": "Burnout", "artist": "Iron Pulse", "duration": "4:10"},
        ],
    },
    {
        "id": "p3",
        "name": "Late Night Jazz",
        "tracks": [
            {"id": "t8", "title": "Blue Hour", "artist": "Miles Ashby", "duration": "5:02"},
            {"id": "t9", "title": "Smoke Rings", "artist": "Nora Quinn", "duration": "4:33"},
        ],
    },
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
        set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h2(class_name="sidebar-title", "Playlists"),
            ul(class_name="playlist-list",
                *[li(
                    key=p["id"],
                    class_name=f"playlist-item{' active' if p['id'] == active_playlist_id else ''}",
                    on_click=lambda p=p: select_playlist(p["id"]),
                    div(class_name="playlist-name", p["name"]),
                    div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
                ) for p in PLAYLISTS],
            ),
        ),
        div(class_name="main-panel",
            h2(class_name="main-title", active_playlist["name"]),
            ul(class_name="track-list",
                *[li(
                    key=t["id"],
                    class_name=f"track-item{' now-playing' if now_playing and now_playing['id'] == t['id'] else ''}",
                    on_click=lambda t=t: select_track(t),
                    span(class_name="track-title", t["title"]),
                    span(class_name="track-artist", t["artist"]),
                    span(class_name="track-duration", t["duration"]),
                ) for t in active_playlist["tracks"]],
            ),
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                now_playing["title"] if now_playing else "No track selected",
            ),
            button(
                class_name="play-toggle",
                on_click=lambda: toggle_play(),
                disabled=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
