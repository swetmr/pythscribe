<!-- exp=baseline-haiku task=macro_tweet_composer cond=psc sample=5 model=claude-haiku-4-5-20251001 cost_usd=0.0181528 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "Alice", "text": "Just launched my new project!", "likes": 42},
        {"id": 2, "author": "Bob", "text": "Learning PythScribe is amazing", "likes": 15},
        {"id": 3, "author": "Carol", "text": "Coffee and code, best combo", "likes": 89},
    ])
    text, set_text = us("")
    next_id, set_next_id = us(4)
    
    char_count = len(text)
    remaining = 280 - char_count
    can_post = text.strip() and char_count <= 280
    
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
        updated = []
        for t in tweets:
            if t["id"] == tweet_id:
                updated.append({**t, "likes": t["likes"] + 1})
            else:
                updated.append(t)
        set_tweets(updated)
    
    def on_key(e):
        if e.key == "Enter" and e.ctrlKey and can_post:
            post()
    
    counter_class = "counter over" if remaining < 0 else "counter"
    
    return div(cn="tweet-feed",
        div(cn="compose-container",
            h2("Compose"),
            textarea(cn="compose-textarea", value=text, 
                    oh=lambda e: set_text(e.target.value),
                    on_key_down=on_key,
                    ph="What's happening!?"),
            div(cn="char-counter",
                span(cn=counter_class, f"{remaining}/280")),
            button(cn="post-button", oc=lambda: post(), dis=not can_post,
                   "Post"),
        ),
        div(cn="tweets-container",
            *[div(cn="tweet", key=t["id"],
                  div(cn="tweet-header",
                      span(cn="author", t["author"])),
                  p(cn="tweet-text", t["text"]),
                  button(cn="like-button", oc=lambda tid=t["id"]: like_tweet(tid),
                         f"❤ {t['likes']}"))
              for t in tweets]
        ),
    )
```
