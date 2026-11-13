# Coin page states

One file per page state, written by hand in the shape the coin views and events have (rc.7). Each
file holds the decoded JSON of the views the page reads (`views`), the `EVENT_JSON` log lines with
their block and time (`events`), the `view_account` answer and access key count of the coin, the
clock the page uses (`now_ns`), and, for the error state, the failure (`fail`). A view in
`missing_views` is one the coin does not have. The web tests turn each file into a chain reader and
check what the page shows.
