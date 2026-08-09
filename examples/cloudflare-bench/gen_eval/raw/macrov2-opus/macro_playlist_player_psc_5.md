<!-- exp=macrov2-opus task=macro_playlist_player cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.044302999999999995 -->
```python
"use client"
R*

def format_duration(seconds):
    m = seconds // 60
    s = seconds % 60
    return f"{m}:{s:02d}"

PLAYLISTS = [
    {
        "id": "focus",
        "name": "Deep Focus",
        "tracks": [
            {"title": "Clarity", "artist": "Nils Frahm", "duration": 214},
            {"title": "Weightless", "artist": "Marconi Union", "duration": 486},
            {"title": "Re:Stacks", "artist": "Bon Iver", "duration": 385},
        ],
    },
    {
        "id": "workout",
        "name": "Workout Pump",
        "tracks": [
            {"title": "Titanium", "artist": "David Guetta", "duration": 245},
            {"title": "Stronger", "artist": "Kanye West", "duration": 312},
            {"title": "Can't Hold Us", "artist": "Macklemore", "duration": 258},
            {"title": "Power", "artist": "Kanye West", "duration": 292},
        ],
    },
    {
        "id": "chill",
        "name": "Evening Chill",
        "tracks": [
            {"title": "Sunset Lover", "artist": "Petit Biscuit", "duration": 235},
            {"title": "Intro", "artist": "The xx", "duration": 127},
            {"title": "Nightcall", "artist": "Kavinsky", "duration": 258},
        ],
    },
]

@c
def PlaylistPlayer():
    active_id, set_active_id = us(PLAYLISTS[0]["id"])
    now_playing, set_now_playing = us(None)
    is_playing, set_is_playing = us(False)

    active = next(p for p in PLAYLISTS if p["id"] == active_id)

    def select_playlist(pid):
        set_active_id(pid)

    def play_track(track):
        set_now_playing(track)
        set_is_playing(True)

    def toggle_play():
        if now_playing:
            set_is_playing(not is_playing)

    return div(cn="playlist-player",
        div(cn="pp-body",
            aside(cn="pp-sidebar",
                h2(cn="pp-sidebar-title", "Playlists"),
                ul(cn="pp-playlist-list",
                    *[li(key=p["id"],
                         cn="pp-playlist-item active" if p["id"] == active_id else "pp-playlist-item",
                         oc=lambda: select_playlist(p["id"]),
                         span(cn="pp-playlist-name", p["name"]),
                         span(cn="pp-track-count", f"{len(p['tracks'])} tracks"))
                      for p in PLAYLISTS]),
            ),
            main(cn="pp-main",
                h2(cn="pp-main-title", active["name"]),
                ul(cn="pp-track-list",
                    *[li(key=i,
                         cn="pp-track active" if now_playing and now_playing["title"] == t["title"] else "pp-track",
                         oc=lambda: play_track(t),
                         span(cn="pp-track-title", t["title"]),
                         span(cn="pp-track-artist", t["artist"]),
                         span(cn="pp-track-duration", format_duration(t["duration"])))
                      for i, t in enumerate(active["tracks"])]),
            ),
        ),
        footer(cn="pp-now-bar",
            div(cn="pp-now-info",
                span(cn="pp-now-label", "Now Playing: "),
                span(cn="pp-now-title", now_playing["title"] if now_playing else "Nothing"),
            ),
            button(cn="pp-play-toggle", dis=now_playing is None, oc=lambda: toggle_play(),
                "Pause" if is_playing else "Play"),
        ),
    )
```
