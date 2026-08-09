<!-- exp=baseline-sonnet task=macro_playlist_player cond=ps sample=1 model=claude-sonnet-5 cost_usd=0.053862 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {"id": 1, "name": "Chill Vibes", "tracks": [
        {"id": 1, "title": "Sunset Drive", "artist": "Nightbird", "duration": "3:24"},
        {"id": 2, "title": "Ocean Breeze", "artist": "Coral Waves", "duration": "4:02"},
        {"id": 3, "title": "Slow Motion", "artist": "Lena Frost", "duration": "2:58"},
    ]},
    {"id": 2, "name": "Workout Mix", "tracks": [
        {"id": 4, "title": "Power Up", "artist": "Ironclad", "duration": "3:10"},
        {"id": 5, "title": "Run Faster", "artist": "Voltage", "duration": "3:45"},
        {"id": 6, "title": "No Limits", "artist": "DJ Kinetic", "duration": "3:33"},
        {"id": 7, "title": "Beast Mode", "artist": "Ironclad", "duration": "2:50"},
    ]},
    {"id": 3, "name": "Focus Flow", "tracks": [
        {"id": 8, "title": "Deep Work", "artist": "Quiet Mind", "duration": "5:12"},
        {"id": 9, "title": "Clarity", "artist": "Still Point", "duration": "4:40"},
    ]},
    {"id": 4, "name": "Road Trip", "tracks": [
        {"id": 10, "title": "Highway Sun", "artist": "Wide Open", "duration": "3:58"},
        {"id": 11, "title": "Miles Away", "artist": "Coral Waves", "duration": "3:21"},
        {"id": 12, "title": "Windows Down", "artist": "Lena Frost", "duration": "3:15"},
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
                span(class_name="now-playing-label", "Now Playing:"),
                span(class_name="now-playing-title", now_playing["title"] if now_playing else "Nothing selected"),
                span(class_name="now-playing-artist", f" - {now_playing['artist']}") if now_playing else None,
            ),
            button(
                class_name="play-pause-btn",
                on_click=lambda: toggle_play(),
                disabled=not now_playing,
                "Pause" if is_playing else "Play",
            ),
        ),
    )
```
