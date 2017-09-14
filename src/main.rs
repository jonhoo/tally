extern crate ansi_term;
#[macro_use]
extern crate clap;
extern crate csv;
extern crate libc;
extern crate time;

use clap::{App, AppSettings, Arg};
use libc::{c_long, getrusage, rusage, suseconds_t, time_t, timeval, RUSAGE_CHILDREN};
use std::process::Command;
use std::process;

fn main() {
    let mut app = App::new("tally")
        .version(crate_version!())
        .about("prettier subsitute for time")
        .long_about(
            "\
             tally runs the specified program `command` with the given arguments. \
             When `command` finishes, time writes a message to standard error giving timing \
             statistics about this program run. These statistics include (i) the elapsed real \
             time between invocation and termination, (ii) the user and system CPU time as \
             returned by getrusage(2), and (iii) other runtime statistics such as peak \
             resident memory usage, number of page faults, etc.",
        )
        .setting(AppSettings::AllowExternalSubcommands)
        .arg(
            Arg::with_name("posix")
                .short("p")
                .long("portability")
                .help("Use the portable output format.")
                .long_help(
                    "\
When in the POSIX locale, use the precise traditional format

  \"real %f\\nuser %f\\nsys %f\\n\"

(with numbers in seconds) where the number of decimals in the
output for %f is unspecified but is sufficient to express the
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

Some of these fields are deprecated (%X, %D, and %W),
and will always be 0.",
                ),
        )
        .arg(
            Arg::with_name("delimited")
                .short("d")
                .long("delimited")
                .takes_value(true)
                //.require_equals(true)
                //.set(ArgSettings::EmptyValues)
                //.default_value(",")
                .help(
                    "Output data in delimited format (CSV with custom delimiter).",
                )
                .next_line_help(true)
                .validator(|v| {
                    use std::ascii::AsciiExt;
                    let mut chars = v.chars();
                    let first = chars.next();
                    if first.is_none() {
                        return Err(String::from("no delimiter given"));
                    }
                    if chars.next().is_some() {
                        return Err(String::from(
                            "only single-character delimiters are supported",
                        ));
                    }
                    let first = first.unwrap();
                    if !first.is_ascii() {
                        return Err(String::from("only ASCII delimiters are supported"));
                    }
                    Ok(())
                })
                .long_help(
                    "\
Outputs timing informating in a machine-readable delimited format.
Each row has a single metric with two columns: field name and
value. The metrics are:

  user: user time (in nanoseconds)
  system: system time (in nanoseconds)
  real: elapsed wall clock time (in nanoseconds)
  peak_mem: max resident memory (in kbytes)
  major_faults: major page faults
  minor_faults: minor page faults",
                ),
        )
        .usage("tally time [options] command [arguments]...")
        .after_help(
            "ARGS:
    command     the command to tally
    arguments   any arguments to pass to <command>",
        );
    let matches = app.clone().get_matches();

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

    let (cmd, cmd_args) = matches.subcommand();
    if cmd.is_empty() {
        app.print_long_help().unwrap();
        process::exit(127);
    }

    let mut command = Command::new(cmd);
    if let Some(args) = cmd_args.unwrap().values_of("") {
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
    let ns: u64 = if let Some(ns) = real_time.num_nanoseconds() {
        ns as u64 - real_time.num_seconds() as u64 * 1_000_000_000
    } else if let Some(us) = real_time.num_microseconds() {
        us as u64 - real_time.num_seconds() as u64 * 1_000_000
    } else {
        let ms = real_time.num_milliseconds();
        ms as u64 - real_time.num_seconds() as u64 * 1_000
    };

    let utime_ns =
        usage.ru_utime.tv_sec as u64 * 1_000_000_000 + usage.ru_utime.tv_usec as u64 * 1_000;
    let stime_ns =
        usage.ru_stime.tv_sec as u64 * 1_000_000_000 + usage.ru_stime.tv_usec as u64 * 1_000;
    let rtime_ns = real_time.num_seconds() as u64 * 1_000_000_000 + ns;
    let ns_to_ms_frac = |ns: u64| {
        format!(
            "{}.{:03}",
            ns / 1_000_000_000,
            (ns % 1_000_000_000) / 1_000_000
        )
    };

    if matches.is_present("posix") {
        eprintln!(
            "real {}\nuser {}\nsys {}",
            ns_to_ms_frac(rtime_ns),
            ns_to_ms_frac(utime_ns),
            ns_to_ms_frac(stime_ns),
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
        pretty_time.push_str(&format!(".{:03}", (rtime_ns % 1_000_000_000) / 1_000_000));
        eprintln!(
            "\
             {}user {}system {}elapsed {:.1}%CPU ({}text+{}data {}max)k\n\
             {}inputs+{}outputs ({}major+{}minor)pagefaults {}swaps",
            ns_to_ms_frac(utime_ns),
            ns_to_ms_frac(stime_ns),
            pretty_time,
            (utime_ns + stime_ns) as f64 / rtime_ns as f64,
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
    } else if let Some(d) = matches.value_of("delimited") {
        use std::io;

        let mut w = csv::WriterBuilder::new();
        // we know there's only one character due to the validator
        let delim = d.chars().next().unwrap();
        // we know there's exactly one ascii character
        let mut b = [0; 1];
        delim.encode_utf8(&mut b);
        w.delimiter(b[0]);
        // write all the stuff to stdout
        let stderr = io::stderr();
        let handle = stderr.lock();
        let mut wrt = w.from_writer(handle);
        wrt.write_field(b"user").unwrap();
        wrt.write_record(&[format!("{}", utime_ns)]).unwrap();
        wrt.write_field(b"system").unwrap();
        wrt.write_record(&[format!("{}", stime_ns)]).unwrap();
        wrt.write_field(b"real").unwrap();
        wrt.write_record(&[format!("{}", rtime_ns)]).unwrap();
        wrt.write_field(b"peak_mem").unwrap();
        wrt.write_record(&[format!("{}", usage.ru_maxrss)]).unwrap();
        wrt.write_field(b"major_faults").unwrap();
        wrt.write_record(&[format!("{}", usage.ru_majflt)]).unwrap();
        wrt.write_field(b"minor_faults").unwrap();
        wrt.write_record(&[format!("{}", usage.ru_minflt)]).unwrap();
        drop(wrt);
        process::exit(exit);
    }

    use ansi_term::Colour;
    let unitc = |u| Colour::White.dimmed().paint(u);
    let unit = |v, u| format!("{}{}", v, unitc(u));

    // we want to show the same units on every row
    let has_h =
        real_time.num_hours() > 0 || usage.ru_utime.tv_sec > 3600 || usage.ru_stime.tv_sec > 3600;
    let has_m = has_h || real_time.num_minutes() > 0 || usage.ru_utime.tv_sec > 60 ||
        usage.ru_stime.tv_sec > 60;
    let has_usec = usage.ru_utime.tv_usec % 1_000 > 0 || usage.ru_stime.tv_usec % 1_000 > 0;
    let has_msec = has_usec || usage.ru_utime.tv_usec > 1_000 || usage.ru_stime.tv_usec > 1_000;

    let pretty_seconds = |mut s| {
        let mut pretty_time = String::new();
        if has_h {
            pretty_time.push_str(&unit(format!("{:>2}", s / 3600), "h "));
        }
        s = s % 3600;
        if has_h || has_m {
            pretty_time.push_str(&unit(format!("{:>2}", s / 60), "m "));
        }
        s = s % 60;
        pretty_time.push_str(&unit(format!("{:>2}", s), "s"));
        pretty_time
    };
    let pretty_time = |t: &timeval| {
        let mut s = pretty_seconds(t.tv_sec);
        let mut usec = t.tv_usec;
        if has_msec {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", usec / 1_000), "ms"));
        }
        usec = usec % 1_000;
        if has_usec {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", usec), "µs"));
        }
        s
    };
    let pretty_time2 = || {
        let mut s = pretty_seconds(real_time.num_seconds());
        let mut ns = ns;
        if has_msec {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", ns / 1_000_000), "ms"));
        }
        if has_usec {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", ns / 1_000), "µs"));
        }
        ns = ns % 1_000;
        if ns != 0 {
            s.push_str(" ");
            s.push_str(&unit(format!("{:>3}", ns), "ns"));
        }
        s
    };
    let pretty_mem = |ks| if ks > 10 * 1024 * 1024 {
        unit(format!("{:.0} ", ks as f64 / 1024f64 / 1024f64), "GB")
    } else if ks > 1024 * 1024 {
        unit(format!("{:.1} ", ks as f64 / 1024f64 / 1024f64), "GB")
    } else if ks > 10 * 1024 {
        unit(format!("{:.0} ", ks as f64 / 1024f64), "MB")
    } else if ks > 1024 {
        unit(format!("{:.1} ", ks as f64 / 1024f64), "MB")
    } else {
        unit(format!("{} ", ks), "kB")
    };

    // now for our new and pretty output format
    eprintln!(
        "\
         {}\n\
         \n\
         {} {}\n\
         {} {}\n\
         {} {}\n\n\
         {} {}\n\
         {} {}, {}\n\
         \n{}",
        Colour::White
            .dimmed()
            .paint(format!("{:-^45}", " [stats] ")),
        Colour::Yellow.paint(format!("{:>15}", "user time:")),
        pretty_time(&usage.ru_utime),
        Colour::Yellow.paint(format!("{:>15}", "system time:")),
        pretty_time(&usage.ru_stime),
        Colour::Yellow.paint(format!("{:>15}", "real time:")),
        pretty_time2(),
        Colour::Yellow.paint(format!("{:>15}", "max memory:")),
        pretty_mem(usage.ru_maxrss),
        Colour::Yellow.paint(format!("{:>15}", "page faults:")),
        unit(format!("{}", usage.ru_majflt), "major"),
        unit(format!("{}", usage.ru_minflt), "minor"),
        Colour::White.dimmed().paint(format!("{:-^45}", "")),
    );
    process::exit(exit);
}
