<!-- exp=baseline-haiku task=macro_tweet_composer cond=psc sample=1 model=claude-haiku-4-5-20251001 cost_usd=0.0483403 -->
```python
"use client"
R*

@c
def TweetFeed():
    seed_tweets = [
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Learning PythScribe today", "likes": 15},
        {"id": 3, "author": "Charlie", "text": "Web development is fun", "likes": 28},
    ]
    
    tweets, set_tweets = us(seed_tweets)
    compose_text, set_compose_text = us("")
    
    char_count = len(compose_text)
    remaining = 280 - char_count
    is_over_limit = char_count > 280
    is_empty = char_count == 0
    
    def post_tweet():
        if not is_empty and not is_over_limit:
            new_tweet = {
                "id": (max([t["id"] for t in tweets]) + 1) if tweets else 1,
                "author": "You",
                "text": compose_text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_compose_text("")
    
    def toggle_like(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(cn="tweet-feed",
        div(cn="compose-section",
            h2("Compose"),
            textarea(
                value=compose_text,
                oh=lambda e: set_compose_text(e.target.value),
                ph="What's happening?!",
                cn="compose-textarea"
            ),
            div(cn="compose-footer",
                span(cn=f"char-count{' over-limit' if is_over_limit else ''}", 
                    f"{remaining} characters remaining"),
                button(
                    oc=post_tweet,
                    dis=is_empty or is_over_limit,
                    cn="post-button",
                    "Post"
                ),
            ),
        ),
        div(cn="tweets-container",
            *[
                div(cn="tweet", key=t["id"],
                    div(cn="tweet-header",
                        span(cn="tweet-author", f"@{t['author']}"),
                    ),
                    p(cn="tweet-text", t["text"]),
                    button(
                        oc=lambda tweet_id=t["id"]: toggle_like(tweet_id),
                        cn="like-button",
                        f"❤️ {t['likes']} Likes"
                    ),
                )
                for t in tweets
            ]
        ),
    )
```
