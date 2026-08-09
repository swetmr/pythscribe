<!-- exp=baseline-sonnet task=macro_playlist_player cond=ps sample=4 model=claude-sonnet-5 cost_usd=0.0362157 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "p1",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drive", "artist": "Nomi Lane", "duration": "3:24"},
            {"id": "t2", "title": "Ocean Breeze", "artist": "Kalo Reef", "duration": "2:58"},
            {"id": "t3", "title": "Slow Motion", "artist": "Vera Solis", "duration": "4:02"},
        ],
    },
    {
        "id": "p2",
        "name": "Workout Mix",
        "tracks": [
            {"id": "t4", "title": "Power Up", "artist": "Grit City", "duration": "3:10"},
            {"id": "t5", "title": "Fast Lane", "artist": "Rex Turbo", "duration": "2:45"},
            {"id": "t6", "title": "No Limits", "artist": "Iron Pulse", "duration": "3:33"},
            {"id": "t7", "title": "Sprint", "artist": "Grit City", "duration": "2:51"},
        ],
    },
    {
        "id": "p3",
        "name": "Focus Flow",
        "tracks": [
            {"id": "t8", "title": "Deep Work", "artist": "Study Lo", "duration": "5:12"},
            {"id": "t9", "title": "Quiet Mind", "artist": "Ambient Sol", "duration": "4:44"},
        ],
    },
    {
        "id": "p4",
        "name": "Throwback Hits",
        "tracks": [
            {"id": "t10", "title": "Retro Nights", "artist": "Neon Echo", "duration": "3:47"},
            {"id": "t11", "title": "Golden Days", "artist": "Vinyl Rose", "duration": "3:15"},
            {"id": "t12", "title": "Rewind", "artist": "Neon Echo", "duration": "2:59"},
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
        if now_playing:
            set_is_playing(not is_playing)

    return div(class_name="playlist-player",
        div(class_name="pp-body",
            div(class_name="pp-sidebar",
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
            div(class_name="pp-main",
                h2(class_name="pp-main-title", active_playlist["name"]),
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
