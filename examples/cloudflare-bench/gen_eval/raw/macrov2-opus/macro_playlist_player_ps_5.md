<!-- exp=macrov2-opus task=macro_playlist_player cond=ps sample=5 model=claude-opus-4-8 cost_usd=0.045391999999999995 -->
```python
"use client"

from pyths.react import component, use_state

PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Sunset Drive", "artist": "Loona", "duration": "3:42"},
            {"title": "Ocean Eyes", "artist": "Marlow", "duration": "4:05"},
            {"title": "Slow Motion", "artist": "Ivy Sound", "duration": "3:18"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Rainfall", "artist": "Aeon", "duration": "5:12"},
            {"title": "Study Hall", "artist": "Ken Waves", "duration": "2:58"},
            {"title": "Quiet Mind", "artist": "Nori", "duration": "4:44"},
            {"title": "Focus Flow", "artist": "Delta", "duration": "3:30"},
        ],
    },
    {
        "id": "party",
        "name": "Party Starters",
        "tracks": [
            {"title": "Neon Nights", "artist": "Vega", "duration": "3:01"},
            {"title": "Bassline", "artist": "DJ Krux", "duration": "3:55"},
            {"title": "Fireworks", "artist": "Halo", "duration": "4:20"},
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
        set_now_playing(None)
        set_is_playing(False)

    def play_track(index):
        set_now_playing(index)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    current_track = active["tracks"][now_playing] if now_playing is not None else None

    return div(class_name="playlist-player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2(class_name="sidebar-title", "Playlists"),
                ul(class_name="playlist-list",
                    *[li(key=pl["id"],
                         class_name="playlist-item active" if pl["id"] == active_id else "playlist-item",
                         on_click=lambda: select_playlist(pl["id"]),
                         span(class_name="playlist-name", pl["name"]),
                         span(class_name="playlist-count", f"{len(pl['tracks'])} tracks"))
                      for pl in PLAYLISTS]),
            ),
            section(class_name="track-panel",
                h2(class_name="panel-title", active["name"]),
                ul(class_name="track-list",
                    *[li(key=i,
                         class_name="track-item playing" if now_playing == i else "track-item",
                         on_click=lambda: play_track(i),
                         span(class_name="track-title", t["title"]),
                         span(class_name="track-artist", t["artist"]),
                         span(class_name="track-duration", t["duration"]))
                      for i, t in enumerate(active["tracks"])]),
            ),
        ),
        div(class_name="now-playing-bar",
            div(class_name="now-playing-info",
                span(class_name="now-playing-label", "Now Playing: "),
                span(class_name="now-playing-title",
                     current_track["title"] if current_track else "Nothing selected")),
            button(class_name="play-toggle",
                   disabled=now_playing is None,
                   on_click=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
