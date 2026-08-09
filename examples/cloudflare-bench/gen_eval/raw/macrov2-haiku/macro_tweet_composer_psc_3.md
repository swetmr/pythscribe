<!-- exp=macrov2-haiku task=macro_tweet_composer cond=psc sample=3 model=claude-haiku-4-5-20251001 cost_usd=0.024460299999999997 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Learning PythScribe is awesome", "likes": 15},
        {"id": 3, "author": "Charlie", "text": "Building cool things today", "likes": 89},
    ])
    
    text, set_text = us("")
    
    remaining = 280 - len(text)
    can_post = len(text) > 0 and remaining >= 0
    
    def post():
        new_tweet = {
            "id": max([t["id"] for t in tweets] + [0]) + 1,
            "author": "You",
            "text": text,
            "likes": 0,
        }
        set_tweets([new_tweet, *tweets])
        set_text("")
    
    def like_tweet(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(cn="tweet-feed",
        div(cn="compose",
            textarea(value=text, oh=lambda e: set_text(e.target.value),
                    ph="What's happening!?", cn="textarea"),
            div(cn="footer",
                span(cn="counter", f"{remaining} / 280"),
                button(oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        div(cn="tweets",
            *[div(key=t["id"], cn="tweet",
                div(cn="author", f"{t['author']}"),
                div(cn="text", t["text"]),
                button(cn="like", oc=lambda: like_tweet(t["id"]), f"❤️ {t['likes']}"),
            ) for t in tweets]
        ),
    )
```
