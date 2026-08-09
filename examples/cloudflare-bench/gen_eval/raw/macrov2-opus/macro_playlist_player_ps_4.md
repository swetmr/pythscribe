<!-- exp=macrov2-opus task=macro_playlist_player cond=ps sample=4 model=claude-opus-4-8 cost_usd=0.048142 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "p1",
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Sunset Drive", "artist": "Lo-Fi Collective", "duration": "3:41"},
            {"title": "Foggy Morning", "artist": "Auraline", "duration": "4:12"},
            {"title": "Slow Tide", "artist": "Marren", "duration": "2:58"},
        ],
    },
    {
        "id": "p2",
        "name": "Focus Flow",
        "tracks": [
            {"title": "Deep Work", "artist": "Nolan Vex", "duration": "5:03"},
            {"title": "Quiet Signal", "artist": "Ostara", "duration": "3:27"},
            {"title": "Steady Hands", "artist": "Kova", "duration": "4:45"},
            {"title": "Clear Skies", "artist": "Ambient Unit", "duration": "3:16"},
        ],
    },
    {
        "id": "p3",
        "name": "Workout Heat",
        "tracks": [
            {"title": "Push Through", "artist": "Volt", "duration": "2:49"},
            {"title": "Red Zone", "artist": "Blackline", "duration": "3:33"},
            {"title": "Final Rep", "artist": "Ironhaus", "duration": "3:08"},
        ],
    },
]


def track_key(playlist_id, index):
    return f"{playlist_id}-{index}"


@component
def PlaylistPlayer():
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active = next((p for p in PLAYLISTS if p["id"] == active_id), PLAYLISTS[0])

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(index):
        track = active["tracks"][index]
        set_now_playing({"key": track_key(active["id"], index), **track})
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    def sidebar_item(playlist):
        classes = "playlist-item"
        if playlist["id"] == active_id:
            classes = "playlist-item active"
        return li(key=playlist["id"], class_name=classes,
            on_click=lambda: select_playlist(playlist["id"]),
            span(class_name="playlist-name", playlist["name"]),
            span(class_name="playlist-count", f"{len(playlist['tracks'])} tracks"),
        )

    def track_row(index, track):
        key = track_key(active["id"], index)
        classes = "track-row"
        if now_playing is not None and now_playing["key"] == key:
            classes = "track-row playing"
        return li(key=key, class_name=classes,
            on_click=lambda: play_track(index),
            span(class_name="track-title", track["title"]),
            span(class_name="track-artist", track["artist"]),
            span(class_name="track-duration", track["duration"]),
        )

    return div(class_name="player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2("Playlists"),
                ul(class_name="playlist-list",
                    *[sidebar_item(p) for p in PLAYLISTS]),
            ),
            main(class_name="main-panel",
                h2(active["name"]),
                ul(class_name="track-list",
                    *[track_row(i, t) for i, t in enumerate(active["tracks"])]),
            ),
        ),
        div(class_name="now-bar",
            div(class_name="now-info",
                (span(class_name="now-title",
                      f"{now_playing['title']} — {now_playing['artist']}")
                 if now_playing is not None
                 else span(class_name="now-title empty", "Nothing playing")),
            ),
            button(class_name="play-toggle", disabled=now_playing is None,
                on_click=lambda: toggle_play(),
                "Pause" if is_playing else "Play"),
        ),
    )
```
