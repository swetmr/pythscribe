<!-- exp=macrov2-opus task=macro_tweet_composer cond=psc sample=5 model=claude-opus-4-8 cost_usd=0.028914000000000002 -->
```python
"use client"
R*

@c
def TweetFeed():
    tweets, set_tweets = us([
        {"id": 1, "author": "@ada", "text": "First tweet on the feed!", "likes": 3},
        {"id": 2, "author": "@grace", "text": "Hello world from PythScribe.", "likes": 7},
        {"id": 3, "author": "@alan", "text": "Thinking about machines.", "likes": 12},
    ])
    text, set_text = us("")

    limit = 280
    remaining = limit - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_id = (tweets[0]["id"] + 1) if tweets else 1
        new_tweet = {"id": new_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t
                    for t in tweets])

    return div(cn="tweet-feed",
        div(cn="compose",
            textarea(
                cn="compose-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
            ),
            div(cn="compose-bar",
                span(cn="char-count" if remaining >= 0 else "char-count over", str(remaining)),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Tweet"),
            ),
        ),
        div(cn="tweet-list",
            *[div(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda: like(t["id"]),
                    f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
