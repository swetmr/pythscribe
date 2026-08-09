<!-- exp=baseline-haiku task=macro_tweet_composer cond=ps sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.026068599999999997 -->
```python
"use client"

from pyths.react import component, use_state

SEED_TWEETS = [
    {
        "id": 1,
        "author": "Alice",
        "text": "Just finished a great book on software architecture!",
        "likes": 42
    },
    {
        "id": 2,
        "author": "Bob",
        "text": "Python is incredibly elegant for data processing.",
        "likes": 28
    },
    {
        "id": 3,
        "author": "Carol",
        "text": "Excited to announce our new open-source project!",
        "likes": 105
    }
]

@component
def TweetFeed():
    text, set_text = use_state("")
    tweets, set_tweets = use_state(SEED_TWEETS[:])
    
    char_limit = 280
    remaining = char_limit - len(text)
    
    def handle_compose_change(e):
        new_text = e.target.value
        if len(new_text) <= char_limit:
            set_text(new_text)
    
    def handle_post():
        if text.strip():
            new_tweet = {
                "id": max([t["id"] for t in tweets], default=0) + 1,
                "author": "You",
                "text": text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
    
    def handle_like(tweet_id):
        updated = []
        for t in tweets:
            if t["id"] == tweet_id:
                updated.append({**t, "likes": t["likes"] + 1})
            else:
                updated.append(t)
        set_tweets(updated)
    
    return div(class_name="tweet-feed",
        div(class_name="compose-section",
            textarea(
                class_name="compose-textarea",
                value=text,
                on_change=handle_compose_change,
                placeholder="What's happening!?",
                rows=4
            ),
            div(class_name="compose-footer",
                span(class_name="char-counter",
                    f"{remaining} characters remaining"
                ),
                button(
                    class_name="post-button",
                    on_click=handle_post,
                    disabled=not text.strip() or remaining < 0,
                    "Post"
                )
            )
        ),
        div(class_name="tweets-list",
            *[div(class_name="tweet",
                  key=t["id"],
                  div(class_name="tweet-header",
                      span(class_name="author", f"@{t['author']}"),
                      span(class_name="timestamp", "just now")
                  ),
                  div(class_name="tweet-text", t["text"]),
                  div(class_name="tweet-footer",
                      button(
                          class_name="like-button",
                          on_click=lambda tid=t["id"]: handle_like(tid),
                          f"♥ {t['likes']}"
                      )
                  )
              ) for t in tweets]
        )
    )
```
