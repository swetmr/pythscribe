<!-- exp=macrov2-opus task=macro_playlist_player cond=ps sample=2 model=claude-opus-4-8 cost_usd=0.044941999999999996 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Ocean Eyes", "artist": "Nova Rae", "duration": "3:20"},
            {"title": "Slow Down", "artist": "The Willows", "duration": "4:05"},
            {"title": "Midnight Drive", "artist": "Kite", "duration": "2:58"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Rainfall", "artist": "Ambient Co", "duration": "5:12"},
            {"title": "Study Loop", "artist": "Nimbus", "duration": "6:44"},
            {"title": "Quiet Room", "artist": "Halo", "duration": "3:37"},
            {"title": "Paper Trails", "artist": "Ferns", "duration": "4:18"},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Pump",
        "tracks": [
            {"title": "Ignite", "artist": "Volt", "duration": "3:01"},
            {"title": "No Brakes", "artist": "Redline", "duration": "2:45"},
            {"title": "Full Send", "artist": "Apex", "duration": "3:33"},
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
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    def is_current(track):
        return now_playing is not None and now_playing["title"] == track["title"]

    return div(class_name="playlist-player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2("Playlists"),
                ul(class_name="playlist-list",
                    *[li(key=pl["id"],
                         class_name="playlist-item active" if pl["id"] == active_id else "playlist-item",
                         on_click=lambda: select_playlist(pl["id"]),
                         span(class_name="playlist-name", pl["name"]),
                         span(class_name="playlist-count", f"{len(pl['tracks'])} tracks"))
                      for pl in PLAYLISTS]),
            ),
            section(class_name="main-panel",
                h2(active["name"]),
                ul(class_name="track-list",
                    *[li(key=t["title"],
                         class_name="track-item playing" if is_current(t) else "track-item",
                         on_click=lambda: play_track(t),
                         span(class_name="track-title", t["title"]),
                         span(class_name="track-artist", t["artist"]),
                         span(class_name="track-duration", t["duration"]))
                      for t in active["tracks"]]),
            ),
        ),
        div(class_name="now-playing-bar",
            span(class_name="now-playing-label",
                f"Now Playing: {now_playing['title']}" if now_playing is not None else "Nothing playing"),
            button(class_name="play-toggle",
                   on_click=lambda: toggle_play(),
                   disabled=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
