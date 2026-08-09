<!-- exp=baseline-sonnet task=macro_tweet_composer cond=psc sample=4 model=claude-sonnet-5 cost_usd=0.0431313 -->
```python
"use client"
R*

MAX_LEN = 280

def make_tweet(id, author, text, likes):
    return {"id": id, "author": author, "text": text, "likes": likes}

SEED = [
    make_tweet(1, "ada", "Hello, world!", 3),
    make_tweet(2, "grace", "Compilers are magic.", 5),
    make_tweet(3, "linus", "Just for fun.", 8),
]

@c
def TweetFeed():
    tweets, set_tweets = us(SEED)
    draft, set_draft = us("")
    next_id, set_next_id = us(len(SEED) + 1)

    remaining = MAX_LEN - len(draft)
    can_post = len(draft.strip()) > 0 and remaining >= 0

    def post():
        if not can_post:
            return
        new_tweet = make_tweet(next_id, "you", draft.strip(), 0)
        set_tweets([new_tweet, *tweets])
        set_next_id(next_id + 1)
        set_draft("")

    def like(tweet_id):
        updated = [{**t, "likes": t["likes"] + 1} if t["id"] == tweet_id else t for t in tweets]
        set_tweets(updated)

    return div(cn="tweet-feed",
        div(cn="composer",
            textarea(
                cn="composer-input",
                value=draft,
                oh=lambda e: set_draft(e.target.value),
                ph="What's happening?",
            ),
            div(cn="composer-footer",
                span(cn="char-count" if remaining >= 0 else "char-count over", f"{remaining}"),
                button(oc=lambda: post(), dis=not can_post, "Post"),
            ),
        ),
        ul(cn="tweet-list",
            *[li(key=t["id"], cn="tweet",
                div(cn="tweet-author", t["author"]),
                div(cn="tweet-text", t["text"]),
                button(cn="like-button", oc=lambda t=t: like(t["id"]), f"♥ {t['likes']}"),
            ) for t in tweets],
        ),
    )
```
