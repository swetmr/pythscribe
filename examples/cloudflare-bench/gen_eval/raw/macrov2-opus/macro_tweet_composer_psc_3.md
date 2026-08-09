<!-- exp=macrov2-opus task=macro_tweet_composer cond=psc sample=3 model=claude-opus-4-8 cost_usd=0.033339 -->
```python
"use client"
R*

SEED = [
    {"id": 3, "author": "@ada", "text": "Just shipped a new compiler pass. Feeling good.", "likes": 12},
    {"id": 2, "author": "@grace", "text": "Debugging is twice as hard as writing the code.", "likes": 34},
    {"id": 1, "author": "@linus", "text": "Talk is cheap. Show me the code.", "likes": 88},
]

LIMIT = 280

@c
def TweetFeed():
    tweets, set_tweets = us(SEED)
    text, set_text = us("")
    next_id, set_next_id = us(4)

    remaining = LIMIT - len(text)
    can_post = len(text.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = {"id": next_id, "author": "@you", "text": text.strip(), "likes": 0}
        set_tweets([new_tweet, *tweets])
        set_next_id(next_id + 1)
        set_text("")

    def like(tid):
        set_tweets([{**t, "likes": t["likes"] + 1} if t["id"] == tid else t for t in tweets])

    return div(cn="tweet-feed",
        div(cn="compose",
            textarea(
                cn="compose-input",
                value=text,
                oh=lambda e: set_text(e.target.value),
                ph="What's happening?",
            ),
            div(cn="compose-footer",
                span(cn=f"char-count{' over' if remaining < 0 else ''}", str(remaining)),
                button(cn="post-btn", oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-btn", oc=lambda: like(t["id"]),
                    f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
