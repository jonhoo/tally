extern crate ansi_term;
#[macro_use]
extern crate clap;
extern crate libc;
extern crate time;

use clap::{App, Arg};
use libc::{c_long, getrusage, rusage, suseconds_t, time_t, timeval, RUSAGE_CHILDREN};
use std::process::Command;
use std::process;

fn main() {
    let matches = App::new("tally")
        .version(crate_version!())
        .about("prettier subsitute for time")
        .arg(
            Arg::with_name("posix")
                .short("p")
                .long("portability")
                .help("Use the portable output format.")
                .long_help(
                    "\
When in the POSIX locale, use the precise traditional format

  \"real %f\\nuser %f\\nsys %f\\n\"

(with numbers in seconds) where the number of decimals in the \
output for %f is unspecified but is sufficient to express the \
clock tick accuracy, and at least one.",
                ),
        )
        .arg(
            Arg::with_name("gnu")
                .short("g")
                .long("gnu")
                .help("Use the GNU time output format.")
                .long_help(
                    "\
Use the precise output format produced by GNU time:

  %Uuser %Ssystem %Eelapsed %PCPU (%Xtext+%Ddata %Mmax)k
  %Iinputs+%Ooutputs (%Fmajor+%Rminor)pagefaults %Wswaps

Some of these fields are deprecated (%X, %D, and %W),\
and will always be 0.",
                ),
        )
        .arg(
            Arg::with_name("command")
                .index(1)
                .required(true)
                .value_name("command")
                .help("Command to fork and execute"),
        )
        .arg(
            Arg::with_name("args")
                .index(2)
                .multiple(true)
                .value_name("arguments")
                .help("Arguments to pass to command, if any"),
        )
        .get_matches();

    let mut usage = rusage {
        ru_utime: timeval {
            tv_sec: 0 as time_t,
            tv_usec: 0 as suseconds_t,
        },
        ru_stime: timeval {
            tv_sec: 0 as time_t,
            tv_usec: 0 as suseconds_t,
        },
        ru_maxrss: 0 as c_long,
        ru_ixrss: 0 as c_long,
        ru_idrss: 0 as c_long,
        ru_isrss: 0 as c_long,
        ru_minflt: 0 as c_long,
        ru_majflt: 0 as c_long,
        ru_nswap: 0 as c_long,
        ru_inblock: 0 as c_long,
        ru_oublock: 0 as c_long,
        ru_msgsnd: 0 as c_long,
        ru_msgrcv: 0 as c_long,
        ru_nsignals: 0 as c_long,
        ru_nvcsw: 0 as c_long,
        ru_nivcsw: 0 as c_long,
    };

    let mut command = Command::new(matches.value_of("command").unwrap());
    if let Some(args) = matches.values_of("args") {
        command.args(args);
    }

    let mut child = match command.spawn() {
        Ok(child) => child,
        Err(e) => {
            use std::io::ErrorKind;
            match e.kind() {
                ErrorKind::NotFound => {
                    process::exit(127);
                }
                ErrorKind::PermissionDenied => {
                    process::exit(126);
                }
                _ => {}
            }
            match e.raw_os_error() {
                Some(e) if e > 0 && e <= 125 => {
                    process::exit(125);
                }
                _ => process::exit(1),
            }
        }
    };

    let start = time::PreciseTime::now();
    let exit = child.wait();
    let end = time::PreciseTime::now();
    let exit = match exit {
        Ok(exit) => {
            match exit.code() {
                Some(exit) => exit,
                None => {
                    // signal
                    1
                }
            }
        }
        Err(_) => 1,
    };

    match unsafe { getrusage(RUSAGE_CHILDREN, (&mut usage) as *mut rusage) } {
        0 => {}
        _ => process::exit(exit),
    }

    let real_time = start.to(end);
    let ns = if let Some(ns) = real_time.num_nanoseconds() {
        ns - real_time.num_seconds() * 1_000_000_000
    } else if let Some(us) = real_time.num_microseconds() {
        us - real_time.num_seconds() * 1_000_000
    } else {
        let ms = real_time.num_milliseconds();
        ms - real_time.num_seconds() * 1_000
    };
    let real_frac = ns as f64 / 1_000_000_000f64;
    let real_frac = format!("{}", real_frac);
    let real_frac = real_frac.trim_left_matches("0.");
    let timeval_secs = |t: &timeval| {
        let frac = format!("{}", t.tv_usec as f64 / 1_000_000f64);
        let frac = frac.trim_left_matches("0.");
        format!("{}.{}", t.tv_sec, frac)
    };

    if matches.is_present("posix") {
        eprintln!(
            "real {}.{}\nuser {}\nsys {}",
            real_time.num_seconds(),
            real_frac,
            timeval_secs(&usage.ru_utime),
            timeval_secs(&usage.ru_stime)
        );
        process::exit(exit);
    } else if matches.is_present("gnu") {
        let mut pretty_time = String::new();
        let mut t = real_time.num_seconds();
        if t / 3600 > 0 {
            pretty_time.push_str(&format!("{}:", t / 3600));
        }
        t = t % 3600;
        pretty_time.push_str(&format!("{}:", t / 60));
        t = t % 60;
        pretty_time.push_str(&format!("{:02}", t));
        pretty_time.push_str(&format!(".{}", real_frac));
        eprintln!(
            "\
             {}user {}system {}elapsed {}%CPU ({}text+{}data {}max)k\n\
             {}inputs+{}outputs ({}major+{}minor)pagefaults {}swaps",
            timeval_secs(&usage.ru_utime),
            timeval_secs(&usage.ru_stime),
            pretty_time,
            // TODO: also count usecs
            (usage.ru_utime.tv_sec + usage.ru_stime.tv_sec) / real_time.num_seconds(),
            0, // deprecated
            0, // deprecated
            usage.ru_maxrss,
            usage.ru_inblock,
            usage.ru_oublock,
            usage.ru_majflt,
            usage.ru_minflt,
            0, // deprecated
        );
        process::exit(exit);
    }

    use ansi_term::Colour;
    let unitc = |u| Colour::White.dimmed().paint(u);
    let unit = |v, u| format!("{}{}", v, unitc(u));

    let pretty_seconds = |mut s| {
        let mut pretty_time = String::new();
        let mut hours = false;
        if s / 3600 > 0 {
            pretty_time.push_str(&unit(format!("{:>2}", s / 3600), "h"));
            hours = true;
        }
        s = s % 3600;
        if s / 60 > 0 || hours {
            pretty_time.push_str(&unit(format!("{:>2}", s / 60), "m"));
        }
        s = s % 60;
        pretty_time.push_str(&unit(format!("{:>2}", s), "s"));
        pretty_time
    };
    let pretty_time = |t: &timeval| {
        let mut s = pretty_seconds(t.tv_sec);
        let mut usec = t.tv_usec;
        if usec > 1_000 {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", t.tv_usec / 1_000), "ms"));
            usec = usec % 1_000;
        }
        if usec > 0 {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", usec), "µs"));
        }
        s
    };
    let pretty_time2 = || {
        let mut s = pretty_seconds(real_time.num_seconds());
        let mut ns = ns;
        if ns > 1_000_000 {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", ns / 1_000_000), "ms"));
            ns = ns % 1_000_000;
        }
        if ns > 1_000 {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", ns / 1_000), "µs"));
            ns = ns % 1_000;
        }
        if ns > 0 {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", ns), "ns"));
        }
        s
    };
    let pretty_mem = |ks| if ks > 10 * 1024 * 1024 {
        unit(format!("{:>3.0} ", ks as f64 / 1024f64 / 1024f64), "GB")
    } else if ks > 1024 * 1024 {
        unit(format!("{:>5.1} ", ks as f64 / 1024f64 / 1024f64), "GB")
    } else if ks > 10 * 1024 {
        unit(format!("{:>3.0} ", ks as f64 / 1024f64), "MB")
    } else if ks > 1024 {
        unit(format!("{:>5.1} ", ks as f64 / 1024f64), "MB")
    } else {
        unit(format!("{:>5} ", ks), "kB")
    };

    // now for our new and pretty output format
    eprintln!(
        "\
         {} {}\n\
         {} {}\n\
         {} {}\n\n\
         {} {}\n\
         {} {}, {}",
        // htop colors
        Colour::Yellow.paint(format!("{:>13}", "user time:")),
        pretty_time(&usage.ru_utime),
        Colour::Yellow.paint(format!("{:>13}", "system time:")),
        pretty_time(&usage.ru_stime),
        Colour::Yellow.paint(format!("{:>13}", "real time:")),
        pretty_time2(),
        Colour::Yellow.paint(format!("{:>13}", "max memory:")),
        pretty_mem(usage.ru_maxrss),
        Colour::Yellow.paint(format!("{:>13}", "page faults:")),
        unit(format!("{}", usage.ru_majflt), "major"),
        unit(format!("{}", usage.ru_minflt), "minor"),
    );
    process::exit(exit);
}
