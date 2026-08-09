<!-- exp=macrov2-haiku task=macro_tweet_composer cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.028386599999999998 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "Alice", "text": "PythScribe is amazing!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Building components is fun", "likes": 15},
        {"id": 3, "author": "Charlie", "text": "React hooks in Python!", "likes": 28},
    ])
    
    text, set_text = us("")
    next_id, set_next_id = us(4)
    
    char_count = len(text)
    remaining = 280 - char_count
    can_post = 0 < char_count <= 280
    
    def post():
        new_tweet = {
            "id": next_id,
            "author": "You",
            "text": text,
            "likes": 0
        }
        set_tweets([new_tweet, *tweets])
        set_text("")
        set_next_id(next_id + 1)
    
    def like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?!",
                cn="compose-textarea"
            ),
            div(cn="composer-footer",
                span(cn=f"char-counter {'over-limit' if remaining < 0 else ''}", 
                     f"{remaining}"),
                button(
                    oc=lambda: post(),
                    dis=not can_post,
                    cn="post-button",
                    "Post"
                )
            )
        ),
        div(cn="tweets-list",
            *[div(cn="tweet", key=t["id"],
                div(cn="tweet-header",
                    span(cn="author", t["author"])
                ),
                div(cn="tweet-text", t["text"]),
                div(cn="tweet-footer",
                    button(
                        oc=lambda: like(t["id"]),
                        cn="like-button",
                        f"♥ {t['likes']}"
                    )
                )
            ) for t in tweets]
        )
    )
```
