<!-- exp=baseline-sonnet task=macro_playlist_player cond=ps sample=3 model=claude-sonnet-5 cost_usd=0.0345207 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": 1,
        "name": "Chill Vibes",
        "tracks": [
            {"id": 101, "title": "Ocean Drive", "artist": "Nova Reyes", "duration": "3:24"},
            {"id": 102, "title": "Late Night Rain", "artist": "Kai Sundo", "duration": "4:02"},
            {"id": 103, "title": "Slow Burn", "artist": "Wilder Fields", "duration": "2:58"},
        ],
    },
    {
        "id": 2,
        "name": "Workout Mix",
        "tracks": [
            {"id": 201, "title": "Overdrive", "artist": "Sable Rush", "duration": "3:11"},
            {"id": 202, "title": "Iron Pulse", "artist": "Marcus Vale", "duration": "3:45"},
            {"id": 203, "title": "Redline", "artist": "Nova Reyes", "duration": "2:39"},
            {"id": 204, "title": "Push Harder", "artist": "Deja Cross", "duration": "3:30"},
        ],
    },
    {
        "id": 3,
        "name": "Focus Flow",
        "tracks": [
            {"id": 301, "title": "Quiet Static", "artist": "Elin Marsh", "duration": "4:15"},
            {"id": 302, "title": "Glass Halls", "artist": "Theo Windham", "duration": "3:52"},
            {"id": 303, "title": "Paper Trails", "artist": "Elin Marsh", "duration": "3:07"},
        ],
    },
    {
        "id": 4,
        "name": "Throwback Hits",
        "tracks": [
            {"id": 401, "title": "Neon Memory", "artist": "Case & Fry", "duration": "3:33"},
            {"id": 402, "title": "Static Youth", "artist": "The Halfmoons", "duration": "4:08"},
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
        if now_playing is not None:
            set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="pp-body",
            nav(class_name="pp-sidebar",
                h2(class_name="pp-sidebar-title", "Playlists"),
                ul(class_name="pp-playlist-list",
                    *[li(
                        key=p["id"],
                        class_name=f"pp-playlist-item{' active' if p['id'] == active_playlist_id else ''}",
                        on_click=lambda p=p: select_playlist(p["id"]),
                        div(class_name="pp-playlist-name", p["name"]),
                        div(class_name="pp-playlist-count", f"{len(p['tracks'])} tracks"),
                    ) for p in PLAYLISTS],
                ),
            ),
            main(class_name="pp-main",
                h1(class_name="pp-main-title", active_playlist["name"]),
                ul(class_name="pp-track-list",
                    *[li(
                        key=t["id"],
                        class_name=f"pp-track-item{' now-playing' if now_playing and t['id'] == now_playing['id'] else ''}",
                        on_click=lambda t=t: select_track(t),
                        span(class_name="pp-track-title", t["title"]),
                        span(class_name="pp-track-artist", t["artist"]),
                        span(class_name="pp-track-duration", t["duration"]),
                    ) for t in active_playlist["tracks"]],
                ),
            ),
        ),
        div(class_name="pp-bottom-bar",
            div(class_name="pp-bottom-info",
                span(class_name="pp-bottom-title",
                     now_playing["title"] if now_playing else "Nothing playing"),
                span(class_name="pp-bottom-artist",
                     now_playing["artist"] if now_playing else ""),
            ),
            button(
                class_name="pp-play-toggle",
                on_click=lambda: toggle_play(),
                disabled=now_playing is None,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
