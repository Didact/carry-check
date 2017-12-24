macro_rules! bind {

    ( [ $($rest:tt)+ ], $fun:expr ) => {{
        bind_helper!($($rest)+, $fun)
    }};

}

macro_rules! bind_helper {
    ( $i:ident, $($rest:tt)+) => {{
        let $i = $i;
        bind_helper!($($rest)+)
    }};

    ( $i:ident = $e:expr, $($rest:tt)+) => {{
        let $i = $e;
        bind_helper!($($rest)+)
    }};

    ( $fun:expr ) => {
        move |other| {
            $fun(other)
        }
    };
}
