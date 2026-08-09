<!-- exp=macrov2-opus task=macro_playlist_player cond=psc sample=2 model=claude-opus-4-8 cost_usd=0.043803 -->
```python
"use client"
R*

PLAYLISTS = [
    {"id": "chill", "name": "Chill Vibes", "tracks": [
        {"title": "Sunset Drive", "artist": "Lo-Fi Collective", "duration": "3:24"},
        {"title": "Ocean Breeze", "artist": "Calm Waves", "duration": "4:01"},
        {"title": "Golden Hour", "artist": "Mellow Mind", "duration": "2:58"},
    ]},
    {"id": "focus", "name": "Deep Focus", "tracks": [
        {"title": "Concentrate", "artist": "Study Beats", "duration": "5:12"},
        {"title": "Flow State", "artist": "Ambient Works", "duration": "6:30"},
    ]},
    {"id": "party", "name": "Party Mix", "tracks": [
        {"title": "Night Lights", "artist": "The Groove", "duration": "3:45"},
        {"title": "Dance Floor", "artist": "Neon Pulse", "duration": "3:10"},
        {"title": "Turn It Up", "artist": "Bass Kings", "duration": "4:22"},
        {"title": "Encore", "artist": "The Groove", "duration": "3:33"},
    ]},
]


def find_playlist(pid):
    for p in PLAYLISTS:
        if p["id"] == pid:
            return p
    return PLAYLISTS[0]


@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = find_playlist(active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing is not None:
            set_is_playing(not is_playing)

    def track_class(track):
        if now_playing is not None and now_playing["title"] == track["title"]:
            return "track track-active"
        return "track"

    return div(cn="playlist-player",
        div(cn="player-body",
            aside(cn="sidebar",
                h2("Your Library"),
                ul(cn="playlist-list",
                    *[li(key=p["id"],
                         cn="playlist-item playlist-item-active" if p["id"] == active_id else "playlist-item",
                         oc=lambda: select_playlist(p["id"]),
                         span(cn="playlist-name", p["name"]),
                         span(cn="playlist-count", f"{len(p['tracks'])} tracks"))
                      for p in PLAYLISTS]),
            ),
            main(cn="main-panel",
                h1(cn="panel-title", active["name"]),
                ul(cn="track-list",
                    *[li(key=t["title"],
                         cn=track_class(t),
                         oc=lambda: play_track(t),
                         span(cn="track-title", t["title"]),
                         span(cn="track-artist", t["artist"]),
                         span(cn="track-duration", t["duration"]))
                      for t in active["tracks"]]),
            ),
        ),
        footer(cn="now-bar",
            div(cn="now-info",
                span(cn="now-label", "Now Playing:"),
                span(cn="now-title",
                     now_playing["title"] if now_playing is not None else "Nothing playing")),
            button(cn="play-toggle",
                   dis=now_playing is None,
                   oc=lambda: toggle_play(),
                   "Pause" if is_playing else "Play"),
        ),
    )
```
