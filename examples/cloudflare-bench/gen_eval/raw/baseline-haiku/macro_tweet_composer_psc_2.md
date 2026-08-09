<!-- exp=baseline-haiku task=macro_tweet_composer cond=psc sample=2 model=claude-haiku-4-5-20251001 cost_usd=0.0383178 -->
```python
"use client"
R*

@c
def TweetFeed():
    initial_tweets = [
        {"id": 1, "author": "Alice", "text": "Building cool stuff with PythScribe!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "React components are fun", "likes": 15},
        {"id": 3, "author": "Carol", "text": "Just deployed a new feature", "likes": 28},
    ]
    
    tweets, set_tweets = us(initial_tweets)
    text, set_text = us("")
    
    char_count = len(text)
    remaining = 280 - char_count
    over_limit = char_count > 280
    empty = char_count == 0
    can_post = not empty and not over_limit
    
    def post():
        if can_post:
            new_tweet = {
                "id": (max([t["id"] for t in tweets]) + 1) if tweets else 1,
                "author": "You",
                "text": text,
                "likes": 0,
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
    
    def like_tweet(tweet_id):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets])
    
    counter_cn = "char-counter" + (" over-limit" if over_limit else "")
    
    return div(cn="tweet-feed",
        div(cn="compose-section",
            h2("Compose"),
            textarea(
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's on your mind?",
                cn="compose-textarea",
                rows=4,
            ),
            div(cn="compose-footer",
                p(cn=counter_cn, f"{remaining} characters remaining"),
                button(
                    oc=lambda: post(),
                    dis=not can_post,
                    cn="post-button",
                    "Post",
                ),
            ),
        ),
        div(cn="feed",
            *[Tweet(key=t["id"], tweet=t, on_like=lambda tweet_id=t["id"]: like_tweet(tweet_id)) for t in tweets],
        ),
    )

@c
def Tweet(tweet, on_like):
    return div(cn="tweet",
        div(cn="tweet-header",
            span(cn="author", tweet["author"]),
        ),
        p(cn="tweet-text", tweet["text"]),
        div(cn="tweet-actions",
            button(oc=on_like, cn="like-button",
                f"♥ {tweet['likes']}"),
        ),
    )
```
