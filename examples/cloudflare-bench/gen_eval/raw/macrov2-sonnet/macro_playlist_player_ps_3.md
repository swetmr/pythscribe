<!-- exp=macrov2-sonnet task=macro_playlist_player cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0370104 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "p1",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drive", "artist": "Nova Lane", "duration": "3:24"},
            {"id": "t2", "title": "Ocean Breeze", "artist": "Kira Sol", "duration": "2:58"},
            {"id": "t3", "title": "Slow Motion", "artist": "Dusk Choir", "duration": "4:02"},
        ],
    },
    {
        "id": "p2",
        "name": "Workout Mix",
        "tracks": [
            {"id": "t4", "title": "Iron Pulse", "artist": "Grit Machine", "duration": "3:15"},
            {"id": "t5", "title": "Overdrive", "artist": "Volt Runner", "duration": "3:47"},
            {"id": "t6", "title": "Sprint", "artist": "Pace Setter", "duration": "2:41"},
            {"id": "t7", "title": "Heavy Load", "artist": "Grit Machine", "duration": "3:33"},
        ],
    },
    {
        "id": "p3",
        "name": "Late Night Jazz",
        "tracks": [
            {"id": "t8", "title": "Blue Hour", "artist": "Miles Ortega", "duration": "5:12"},
            {"id": "t9", "title": "Smoke & Neon", "artist": "The Velvet Set", "duration": "4:28"},
        ],
    },
    {
        "id": "p4",
        "name": "Road Trip",
        "tracks": [
            {"id": "t10", "title": "Highway Song", "artist": "Cross Country", "duration": "3:50"},
            {"id": "t11", "title": "Open Sky", "artist": "Nova Lane", "duration": "3:09"},
            {"id": "t12", "title": "Dusty Roads", "artist": "The Wanderers", "duration": "4:15"},
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
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active_playlist = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def select_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="pp-body",
            aside(class_name="pp-sidebar",
                h3("Playlists"),
                ul(class_name="pp-playlist-list",
                    *[li(
                        key=p["id"],
                        class_name=f"pp-playlist-item{' active' if p['id'] == active_id else ''}",
                        on_click=lambda p=p: select_playlist(p["id"]),
                        div(class_name="pp-playlist-name", p["name"]),
                        div(class_name="pp-playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in PLAYLISTS],
                ),
            ),
            main(class_name="pp-main",
                h2(active_playlist["name"]),
                ul(class_name="pp-track-list",
                    *[li(
                        key=t["id"],
                        class_name=f"pp-track-item{' now-playing' if now_playing and now_playing['id'] == t['id'] else ''}",
                        on_click=lambda t=t: select_track(t),
                        span(class_name="pp-track-title", t["title"]),
                        span(class_name="pp-track-artist", t["artist"]),
                        span(class_name="pp-track-duration", t["duration"]),
                    ) for t in active_playlist["tracks"]],
                ),
            ),
        ),
        div(class_name="pp-bottom-bar",
            div(class_name="pp-now-playing-info",
                span(class_name="pp-now-playing-label", "Now Playing:"),
                span(class_name="pp-now-playing-title",
                     now_playing["title"] if now_playing else "Nothing selected"),
            ),
            button(
                class_name="pp-play-toggle",
                disabled=now_playing is None,
                on_click=lambda: toggle_play(),
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
