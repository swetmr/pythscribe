<!-- exp=macrov2-haiku task=macro_tweet_composer cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.030305299999999993 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "PythScribe is amazing", "likes": 89},
        {"id": 3, "author": "Charlie", "text": "Building cool things today", "likes": 15},
    ])
    
    text, set_text = us("")
    next_id, set_next_id = us(4)
    
    char_limit = 280
    char_count = len(text)
    remaining = char_limit - char_count
    is_over_limit = char_count > char_limit
    is_empty = char_count == 0
    can_post = not is_empty and not is_over_limit
    
    def post():
        if can_post:
            new_tweet = {
                "id": next_id,
                "author": "You",
                "text": text,
                "likes": 0
            }
            set_tweets([new_tweet, *tweets])
            set_text("")
            set_next_id(next_id + 1)
    
    def like_tweet(tweet_id):
        updated = [
            {**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t
            for t in tweets
        ]
        set_tweets(updated)
    
    return div(cn="tweet-feed",
        div(cn="compose-section",
            textarea(cn="compose-textarea",
                     value=text,
                     oh=lambda e: set_text(e.target.value),
                     ph="What's happening?!"),
            div(cn="compose-footer",
                div(cn="char-counter",
                    f"{remaining}/280" if not is_over_limit else f"{char_count}/280 (over)"),
                button(cn="post-button",
                       oc=lambda: post(),
                       dis=not can_post,
                       "Post")
            )
        ),
        ul(cn="tweets-list",
           *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                div(cn="tweet-actions",
                    button(cn="like-button",
                           oc=lambda: like_tweet(t["id"]),
                           f"♥ {t['likes']}")))
             for t in tweets]
        )
    )
```
