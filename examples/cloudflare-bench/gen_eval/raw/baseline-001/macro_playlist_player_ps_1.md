<!-- exp=baseline-001 task=macro_playlist_player cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.08489050000000001 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"id": "t1", "title": "Sunset Drive", "artist": "Lo-Fi Collective", "duration": "3:42"},
            {"id": "t2", "title": "Ocean Breeze", "artist": "Calm Waters", "duration": "4:05"},
            {"id": "t3", "title": "Midnight Rain", "artist": "Soft Focus", "duration": "3:18"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"id": "t4", "title": "Flow State", "artist": "Neural Beats", "duration": "5:12"},
            {"id": "t5", "title": "Clarity", "artist": "Study Sessions", "duration": "4:47"},
            {"id": "t6", "title": "Concentrate", "artist": "Deep Work", "duration": "6:01"},
            {"id": "t7", "title": "Quiet Mind", "artist": "Ambient Room", "duration": "4:30"},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Energy",
        "tracks": [
            {"id": "t8", "title": "Power Up", "artist": "Adrenaline", "duration": "2:58"},
            {"id": "t9", "title": "Sprint", "artist": "High Tempo", "duration": "3:22"},
            {"id": "t10", "title": "Max Effort", "artist": "Iron Pulse", "duration": "3:47"},
        ],
    },
]


def find_playlist(playlist_id):
    for pl in PLAYLISTS:
        if pl["id"] == playlist_id:
            return pl
    return PLAYLISTS[0]


def find_track(playlist, track_id):
    for track in playlist["tracks"]:
        if track["id"] == track_id:
            return track
    return None


@component
def PlaylistPlayer():
    active_id, set_active_id = use_state(PLAYLISTS[0]["id"])
    now_playing_id, set_now_playing_id = use_state(None)
    is_playing, set_is_playing = use_state(False)

    active_playlist = find_playlist(active_id)
    now_playing = find_track(active_playlist, now_playing_id)

    def select_playlist(playlist_id):
        set_active_id(playlist_id)
        set_now_playing_id(None)
        set_is_playing(False)

    def play_track(track_id):
        set_now_playing_id(track_id)
        set_is_playing(True)

    def toggle_play():
        if now_playing_id is not None:
            set_is_playing(not is_playing)

    def sidebar_item(pl):
        classes = "playlist-item"
        if pl["id"] == active_id:
            classes = "playlist-item active"
        return li(key=pl["id"], class_name=classes,
                  on_click=lambda: select_playlist(pl["id"]),
            span(class_name="playlist-name", pl["name"]),
            span(class_name="playlist-count", f"{len(pl['tracks'])} tracks"),
        )

    def track_row(track):
        classes = "track-row"
        if track["id"] == now_playing_id:
            classes = "track-row now-playing"
        return li(key=track["id"], class_name=classes,
                  on_click=lambda: play_track(track["id"]),
            span(class_name="track-title", track["title"]),
            span(class_name="track-artist", track["artist"]),
            span(class_name="track-duration", track["duration"]),
        )

    return div(class_name="playlist-player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2(class_name="sidebar-heading", "Playlists"),
                ul(class_name="playlist-list",
                    *[sidebar_item(pl) for pl in PLAYLISTS]),
            ),
            main(class_name="main-panel",
                h2(class_name="main-heading", active_playlist["name"]),
                ul(class_name="track-list",
                    *[track_row(track) for track in active_playlist["tracks"]]),
            ),
        ),
        div(class_name="now-bar",
            div(class_name="now-info",
                span(class_name="now-label", "Now Playing"),
                span(class_name="now-title",
                     now_playing["title"] if now_playing else "Nothing selected"),
                span(class_name="now-artist",
                     now_playing["artist"] if now_playing else ""),
            ),
            button(class_name="play-toggle",
                   disabled=now_playing is None,
                   on_click=lambda: toggle_play(),
                "Pause" if is_playing else "Play"),
        ),
    )
```
