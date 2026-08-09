<!-- exp=macrov2-haiku task=macro_tweet_composer cond=psc sample=4 model=claude-haiku-4-5-20251001 cost_usd=0.027235299999999997 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 5},
        {"id": 2, "author": "Bob", "text": "React is amazing", "likes": 12},
        {"id": 3, "author": "Charlie", "text": "Building cool stuff with PythScribe", "likes": 3},
    ])
    
    text, set_text = us("")
    
    char_count = len(text)
    remaining = 280 - char_count
    is_valid = 0 < char_count <= 280
    
    def post():
        if is_valid:
            new_tweet = {
                "id": max(t["id"] for t in tweets) + 1 if tweets else 1,
                "author": "You",
                "text": text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
    
    def toggle_like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(cn="tweet-feed",
        div(cn="compose-section",
            textarea(
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening!",
                cn="compose-textarea"
            ),
            div(cn="compose-footer",
                span(cn=f"char-count {'limit-exceeded' if remaining < 0 else ''}", 
                     f"{remaining}"),
                button(
                    oc=lambda: post(),
                    dis=not is_valid,
                    cn="post-button",
                    "Post"
                )
            )
        ),
        div(cn="tweets-list",
            *[div(key=t["id"], cn="tweet",
                div(cn="tweet-header",
                    span(cn="author", t["author"]),
                ),
                div(cn="tweet-text", t["text"]),
                div(cn="tweet-actions",
                    button(
                        oc=lambda: toggle_like(t["id"]),
                        cn="like-button",
                        f"♥ {t['likes']}"
                    )
                )
            ) for t in tweets]
        )
    )
```
