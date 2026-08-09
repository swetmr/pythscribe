<!-- exp=macrov2-sonnet task=macro_playlist_player cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.05333039999999999 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": 1,
        "name": "Chill Vibes",
        "tracks": [
            {"id": 101, "title": "Sunset Drift", "artist": "Nora Lane", "duration": 214},
            {"id": 102, "title": "Slow Tide", "artist": "Marin Cole", "duration": 187},
            {"id": 103, "title": "Paper Clouds", "artist": "Ezra Vale", "duration": 231},
            {"id": 104, "title": "Quiet Static", "artist": "Nora Lane", "duration": 198},
        ],
    },
    {
        "id": 2,
        "name": "Workout Mix",
        "tracks": [
            {"id": 201, "title": "Iron Pulse", "artist": "The Flarelights", "duration": 176},
            {"id": 202, "title": "Redline", "artist": "Kobe Aster", "duration": 165},
            {"id": 203, "title": "Overdrive", "artist": "The Flarelights", "duration": 203},
        ],
    },
    {
        "id": 3,
        "name": "Late Night Focus",
        "tracks": [
            {"id": 301, "title": "Glass Corridor", "artist": "Yuen Park", "duration": 254},
            {"id": 302, "title": "Static Bloom", "artist": "Ilse Corvin", "duration": 222},
            {"id": 303, "title": "Amber Halls", "artist": "Yuen Park", "duration": 241},
            {"id": 304, "title": "Low Signal", "artist": "Ilse Corvin", "duration": 210},
            {"id": 305, "title": "Faint Orbit", "artist": "Yuen Park", "duration": 196},
        ],
    },
]


def format_duration(seconds):
    minutes = seconds // 60
    secs = seconds % 60
    return f"{minutes}:{secs:02d}"


@component
def PlaylistPlayer():
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active_playlist = [p for p in PLAYLISTS if p["id"] == active_id][0]

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(t):
        set_now_playing(t)
        set_is_playing(True)

    def toggle_play():
        set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="sidebar",
            h2("Playlists"),
            ul(*[
                li(
                    key=p["id"],
                    class_name="playlist-item active" if p["id"] == active_id else "playlist-item",
                    on_click=lambda p=p: select_playlist(p["id"]),
                    div(class_name="playlist-name", p["name"]),
                    div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
                )
                for p in PLAYLISTS
            ]),
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"]),
            ul(class_name="track-list", *[
                li(
                    key=t["id"],
                    class_name="track-item now-playing" if now_playing and now_playing["id"] == t["id"] else "track-item",
                    on_click=lambda t=t: select_track(t),
                    span(class_name="track-title", t["title"]),
                    span(class_name="track-artist", t["artist"]),
                    span(class_name="track-duration", format_duration(t["duration"])),
                )
                for t in active_playlist["tracks"]
            ]),
        ),
        div(class_name="bottom-bar",
            span(class_name="now-playing-title",
                 now_playing["title"] if now_playing else "No track selected"),
            button(
                on_click=lambda: toggle_play(),
                class_name="play-pause-btn",
                disabled=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
