//! The table-driven test form: one test function per row, the row's input beside its
//! name, and the operation run over each input compared with the row's expectation.

macro_rules! rows {
    ( $operation:expr ; $( $name:ident : $input:expr => $expected:expr ),+ $(,)? ) => {
        $(
            #[test]
            fn $name() {
                let input = $input;
                let shown = format!("{input:?}");
                let operation = $operation;
                let observed = operation(input);
                assert_eq!(observed, $expected, "row `{}` ran over {shown}", stringify!($name));
            }
        )+
    };
}
