<!-- exp=baseline-sonnet task=macro_playlist_player cond=ps sample=2 model=claude-sonnet-5 cost_usd=0.0339357 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "pl1",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drive", "artist": "Nora Lane", "duration": "3:24"},
            {"id": "t2", "title": "Slow Rain", "artist": "Kip Ashford", "duration": "4:01"},
            {"id": "t3", "title": "Late Night Talk", "artist": "Mira Voss", "duration": "2:58"},
        ],
    },
    {
        "id": "pl2",
        "name": "Workout Mix",
        "tracks": [
            {"id": "t4", "title": "Pulse", "artist": "DJ Torque", "duration": "3:12"},
            {"id": "t5", "title": "Iron Grip", "artist": "Kaya Reed", "duration": "3:45"},
            {"id": "t6", "title": "Redline", "artist": "Vex Motor", "duration": "2:50"},
            {"id": "t7", "title": "Overdrive", "artist": "DJ Torque", "duration": "3:33"},
        ],
    },
    {
        "id": "pl3",
        "name": "Focus Flow",
        "tracks": [
            {"id": "t8", "title": "Quiet Desk", "artist": "Wen Park", "duration": "5:10"},
            {"id": "t9", "title": "Deep Work", "artist": "Ilan Cho", "duration": "6:02"},
        ],
    },
    {
        "id": "pl4",
        "name": "Throwback Hits",
        "tracks": [
            {"id": "t10", "title": "Backroad Radio", "artist": "The Marlows", "duration": "3:50"},
            {"id": "t11", "title": "Neon Diner", "artist": "Cass Riley", "duration": "4:15"},
            {"id": "t12", "title": "Faded Denim", "artist": "The Marlows", "duration": "3:29"},
        ],
    },
]


def find_playlist(playlist_id):
    for pl in PLAYLISTS:
        if pl["id"] == playlist_id:
            return pl
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
            h2("Playlists"),
            ul(class_name="playlist-list",
                *[li(key=pl["id"],
                     class_name="playlist-item active" if pl["id"] == active_playlist_id else "playlist-item",
                     on_click=lambda pl=pl: select_playlist(pl["id"]),
                     div(class_name="playlist-name", pl["name"]),
                     div(class_name="playlist-count", f"{len(pl['tracks'])} tracks"),
                  ) for pl in PLAYLISTS],
            ),
        ),
        div(class_name="main-panel",
            h2(active_playlist["name"]),
            ul(class_name="track-list",
                *[li(key=track["id"],
                     class_name="track-item now-playing" if now_playing and track["id"] == now_playing["id"] else "track-item",
                     on_click=lambda track=track: select_track(track),
                     span(class_name="track-title", track["title"]),
                     span(class_name="track-artist", track["artist"]),
                     span(class_name="track-duration", track["duration"]),
                  ) for track in active_playlist["tracks"]],
            ),
        ),
        div(class_name="bottom-bar",
            div(class_name="now-playing-info",
                now_playing["title"] if now_playing else "No track selected",
                span(class_name="now-playing-artist", f" — {now_playing['artist']}") if now_playing else None,
            ),
            button(class_name="play-toggle", on_click=lambda: toggle_play(), disabled=now_playing is None,
                "Pause" if is_playing else "Play"),
        ),
    )
```
