<!-- exp=macrov2-sonnet task=macro_playlist_player cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.0588936 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "p1",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drift", "artist": "Nova Bloom", "duration": "3:24"},
            {"id": "t2", "title": "Slow Tide", "artist": "Marina Wells", "duration": "4:02"},
            {"id": "t3", "title": "Paper Clouds", "artist": "Aster Lane", "duration": "2:58"},
        ],
    },
    {
        "id": "p2",
        "name": "Workout Mix",
        "tracks": [
            {"id": "t4", "title": "Iron Pulse", "artist": "Kade Rush", "duration": "3:11"},
            {"id": "t5", "title": "Overdrive", "artist": "Vex Motion", "duration": "3:47"},
            {"id": "t6", "title": "Sprint Line", "artist": "Toma Rios", "duration": "2:39"},
            {"id": "t7", "title": "Heavy Steps", "artist": "Kade Rush", "duration": "3:55"},
        ],
    },
    {
        "id": "p3",
        "name": "Late Night Focus",
        "tracks": [
            {"id": "t8", "title": "Glass Ceiling", "artist": "Orin Vale", "duration": "5:12"},
            {"id": "t9", "title": "Quiet Circuit", "artist": "Lena Dust", "duration": "4:33"},
        ],
    },
    {
        "id": "p4",
        "name": "Road Trip",
        "tracks": [
            {"id": "t10", "title": "Highway Static", "artist": "Ferris Cole", "duration": "3:20"},
            {"id": "t11", "title": "Dust Roads", "artist": "June Harlow", "duration": "3:48"},
            {"id": "t12", "title": "Open Sky", "artist": "Ferris Cole", "duration": "4:10"},
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
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2("Playlists"),
                ul(class_name="playlist-list",
                    *[li(
                        key=p["id"],
                        class_name=f"playlist-item{' active' if p['id'] == active_id else ''}",
                        on_click=lambda pid=None, p=p: select_playlist(p["id"]),
                        div(class_name="playlist-name", p["name"]),
                        div(class_name="playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in PLAYLISTS],
                ),
            ),
            main(class_name="track-panel",
                h2(active_playlist["name"]),
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
        ),
        div(class_name="now-playing-bar",
            div(class_name="now-playing-info",
                span(class_name="now-playing-label", "Now Playing:"),
                span(class_name="now-playing-title",
                     now_playing["title"] if now_playing else "Nothing selected"),
            ) if now_playing else div(class_name="now-playing-info",
                span(class_name="now-playing-label", "Now Playing:"),
                span(class_name="now-playing-title", "Nothing selected"),
            ),
            button(
                class_name="play-toggle-btn",
                on_click=lambda: toggle_play(),
                disabled=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
