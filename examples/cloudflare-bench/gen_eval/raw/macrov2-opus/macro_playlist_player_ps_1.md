<!-- exp=macrov2-opus task=macro_playlist_player cond=ps sample=1 model=claude-opus-4-8 cost_usd=0.07986399999999999 -->
```python
"use client"

from pyths.react import component, use_state


PLAYLISTS = [
    {
        "id": "chill",
        "name": "Chill Vibes",
        "tracks": [
            {"title": "Sunset Drive", "artist": "Nova Wave", "duration": "3:42"},
            {"title": "Ocean Eyes", "artist": "Marina Lee", "duration": "4:05"},
            {"title": "Slow Motion", "artist": "The Dusk", "duration": "3:18"},
        ],
    },
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Quiet Mind", "artist": "Ambient Co", "duration": "5:12"},
            {"title": "Flow State", "artist": "Keys & Co", "duration": "6:30"},
            {"title": "Rainfall", "artist": "Nature One", "duration": "4:48"},
            {"title": "Study Hall", "artist": "Lofi Kid", "duration": "2:59"},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Boost",
        "tracks": [
            {"title": "Adrenaline", "artist": "Pulse", "duration": "3:01"},
            {"title": "Run It Up", "artist": "DJ Volt", "duration": "3:27"},
            {"title": "No Limits", "artist": "Max Power", "duration": "2:44"},
        ],
    },
]


def find_playlist(pid):
    for p in PLAYLISTS:
        if p["id"] == pid:
            return p
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
        return now_playing is not None and now_playing["title"] == track["title"] and now_playing["artist"] == track["artist"]

    bar_text = f"{now_playing['title']} — {now_playing['artist']}" if now_playing is not None else "Nothing playing"

    return div(class_name="playlist-player",
        div(class_name="player-body",
            aside(class_name="sidebar",
                h2("Playlists"),
                ul(class_name="playlist-list",
                    *[li(key=p["id"],
                         class_name="playlist-item active" if p["id"] == active_id else "playlist-item",
                         on_click=lambda: select_playlist(p["id"]),
                         span(class_name="playlist-name", p["name"]),
                         span(class_name="playlist-count", f"{len(p['tracks'])} tracks"))
                      for p in PLAYLISTS]),
            ),
            main(class_name="track-panel",
                h2(active["name"]),
                ul(class_name="track-list",
                    *[li(key=f"{active['id']}-{i}",
                         class_name="track-item now-playing" if is_current(t) else "track-item",
                         on_click=lambda: play_track(t),
                         span(class_name="track-title", t["title"]),
                         span(class_name="track-artist", t["artist"]),
                         span(class_name="track-duration", t["duration"]))
                      for i, t in enumerate(active["tracks"])]),
            ),
        ),
        footer(class_name="now-playing-bar",
            span(class_name="now-playing-text", bar_text),
            button(class_name="play-toggle",
                   on_click=lambda: toggle_play(),
                   disabled=now_playing is None,
                   "Pause" if is_playing else "Play"),
        ),
    )
```
